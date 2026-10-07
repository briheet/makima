//! `work.db`: one work's agents, notes, voice recordings and transcripts,
//! messages to agents, and an append-only event journal.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, params};
use rusqlite_migration::{M, Migrations};
use sha2::{Digest, Sha256};

use makima_core::{AgentId, AgentRole, PermissionMode, PresetId, WorkKind, WorkspaceId};

use crate::sql::{Text, get, get_opt, text};
use crate::{StoreError, new_id, now};

const MIGRATIONS: &[M<'static>] = &[
    M::up(
        "CREATE TABLE work (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        kind TEXT,
        template TEXT,
        folder TEXT,
        read_only INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL
    );
    CREATE TABLE agents (
        id TEXT PRIMARY KEY,
        position INTEGER NOT NULL,
        name TEXT NOT NULL,
        role TEXT NOT NULL,
        preset TEXT,
        command TEXT,
        session_id TEXT,
        started_at TEXT NOT NULL,
        ended_at TEXT,
        exit_code INTEGER
    );
    CREATE TABLE audio (
        id TEXT PRIMARY KEY,
        path TEXT NOT NULL,
        mime TEXT NOT NULL,
        duration_ms INTEGER,
        sha256 TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE TABLE transcripts (
        id TEXT PRIMARY KEY,
        audio_id TEXT NOT NULL REFERENCES audio(id),
        provider TEXT NOT NULL,
        language TEXT,
        text TEXT NOT NULL,
        created_at TEXT NOT NULL
    );
    CREATE TABLE notes (
        id TEXT PRIMARY KEY,
        question TEXT,
        text TEXT NOT NULL,
        audio_id TEXT REFERENCES audio(id),
        created_at TEXT NOT NULL
    );
    CREATE TABLE messages (
        id TEXT PRIMARY KEY,
        text TEXT NOT NULL,
        audio_id TEXT REFERENCES audio(id),
        created_at TEXT NOT NULL
    );
    CREATE TABLE message_recipients (
        message_id TEXT NOT NULL REFERENCES messages(id),
        agent_id TEXT NOT NULL,
        PRIMARY KEY (message_id, agent_id)
    );
    CREATE TABLE events (
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        at TEXT NOT NULL,
        agent_id TEXT,
        kind TEXT NOT NULL,
        detail TEXT
    );",
    ),
    // Permission modes (manual, accept-edits, plan, auto, bypass): the work's
    // choice and the mode each agent started in.
    M::up(
        "ALTER TABLE work ADD COLUMN permission TEXT;
    ALTER TABLE agents ADD COLUMN permission TEXT;",
    ),
];

/// What's needed to create a work. Build with [`NewWork::new`] and the
/// `with_*` methods.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NewWork {
    pub id: WorkspaceId,
    pub name: String,
    pub kind: Option<WorkKind>,
    pub template: Option<String>,
    pub folder: Option<String>,
    pub read_only: bool,
    pub permission: Option<PermissionMode>,
}

impl NewWork {
    #[must_use]
    pub fn new(id: WorkspaceId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            kind: None,
            template: None,
            folder: None,
            read_only: false,
            permission: None,
        }
    }

    #[must_use]
    pub fn with_kind(mut self, kind: Option<WorkKind>) -> Self {
        self.kind = kind;
        self
    }

    #[must_use]
    pub fn with_template(mut self, template: Option<String>) -> Self {
        self.template = template;
        self
    }

    #[must_use]
    pub fn with_folder(mut self, folder: Option<String>) -> Self {
        self.folder = folder;
        self
    }

    #[must_use]
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// The permission mode chosen for the whole work.
    #[must_use]
    pub fn with_permission(mut self, permission: Option<PermissionMode>) -> Self {
        self.permission = permission;
        self
    }
}

/// The `work` row.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorkRow {
    pub id: WorkspaceId,
    pub name: String,
    pub kind: Option<WorkKind>,
    pub template: Option<String>,
    pub folder: Option<String>,
    pub read_only: bool,
    pub permission: Option<PermissionMode>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AgentRow {
    pub id: AgentId,
    /// 0 is the lead.
    pub position: u32,
    pub name: String,
    pub role: AgentRole,
    pub preset: Option<PresetId>,
    pub command: Option<String>,
    /// The program's own session id (Claude Code's), for resuming.
    pub session_id: Option<String>,
    /// The permission mode it started in.
    pub permission: Option<PermissionMode>,
}

impl AgentRow {
    /// An agent at `position` (0 is the lead).
    #[must_use]
    pub fn new(id: AgentId, position: u32, name: impl Into<String>, role: AgentRole) -> Self {
        Self {
            id,
            position,
            name: name.into(),
            role,
            preset: None,
            command: None,
            session_id: None,
            permission: None,
        }
    }

    /// The preset, command line and program session it was started with.
    #[must_use]
    pub fn started_with(
        mut self,
        preset: Option<PresetId>,
        command: Option<String>,
        session_id: Option<String>,
    ) -> Self {
        self.preset = preset;
        self.command = command;
        self.session_id = session_id;
        self
    }

    /// The permission mode it started in.
    #[must_use]
    pub fn with_permission(mut self, permission: Option<PermissionMode>) -> Self {
        self.permission = permission;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AudioRow {
    pub id: String,
    pub path: String,
    pub duration_ms: Option<u64>,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NoteRow {
    pub id: String,
    /// The brief question it answers; `None` for a free note.
    pub question: Option<String>,
    pub text: String,
    pub audio_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MessageRow {
    pub id: String,
    pub text: String,
    pub recipients: Vec<AgentId>,
    pub audio_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct EventRow {
    pub seq: i64,
    pub at: String,
    pub agent_id: Option<AgentId>,
    pub kind: String,
    pub detail: Option<String>,
}

/// One work's folder and database. Safe to share between threads; writes
/// are serialised by its lock.
#[derive(Debug)]
pub struct WorkDb {
    dir: PathBuf,
    db: Mutex<Connection>,
}

/// Opens a database file with Makima's settings: WAL, foreign keys, and a
/// busy timeout so the CLI can read while the app writes.
pub(crate) fn connect(path: &Path) -> Result<Connection, StoreError> {
    let connection = Connection::open(path)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", true)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(connection)
}

impl WorkDb {
    /// Creates `dir` with a new database for `new`.
    ///
    /// # Errors
    /// [`StoreError`] on I/O or database failure.
    pub fn create(dir: &Path, new: &NewWork) -> Result<Self, StoreError> {
        std::fs::create_dir_all(dir.join("audio")).map_err(|e| StoreError::io(dir, e))?;
        let work = Self::open(dir)?;
        work.db().execute(
            "INSERT INTO work (id, name, kind, template, folder, read_only, permission, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                Text(new.id),
                new.name,
                text(new.kind.as_ref()),
                new.template,
                new.folder,
                new.read_only,
                text(new.permission.as_ref()),
                now()
            ],
        )?;
        work.event(None, "work.created", None)?;
        Ok(work)
    }

    /// Opens an existing work folder, migrating its database if needed.
    ///
    /// # Errors
    /// [`StoreError`] on I/O or database failure.
    pub fn open(dir: &Path) -> Result<Self, StoreError> {
        let mut connection = connect(&dir.join("work.db"))?;
        Migrations::from_slice(MIGRATIONS).to_latest(&mut connection)?;
        Ok(Self {
            dir: dir.to_owned(),
            db: Mutex::new(connection),
        })
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where the brief is written.
    #[must_use]
    pub fn brief_path(&self) -> PathBuf {
        self.dir.join("brief.md")
    }

    /// # Errors
    /// [`StoreError`] on database failure, or if the work row is missing.
    pub fn work(&self) -> Result<WorkRow, StoreError> {
        Ok(self.db().query_row(
            "SELECT id, name, kind, template, folder, read_only, permission, created_at FROM work LIMIT 1",
            [],
            |r| {
                Ok(WorkRow {
                    id: get(r, 0)?,
                    name: r.get(1)?,
                    kind: get_opt(r, 2)?,
                    template: r.get(3)?,
                    folder: r.get(4)?,
                    read_only: r.get(5)?,
                    permission: get_opt(r, 6)?,
                    created_at: r.get(7)?,
                })
            },
        )?)
    }

    /// Writes `brief.md` and returns its path.
    ///
    /// # Errors
    /// [`StoreError::Io`].
    pub fn write_brief(&self, markdown: &str) -> Result<PathBuf, StoreError> {
        let path = self.brief_path();
        std::fs::write(&path, markdown).map_err(|e| StoreError::io(&path, e))?;
        Ok(path)
    }

    // MARK: Agents

    /// # Errors
    /// [`StoreError::Database`].
    pub fn add_agent(&self, agent: &AgentRow) -> Result<(), StoreError> {
        self.db().execute(
            "INSERT OR REPLACE INTO agents (id, position, name, role, preset, command, session_id, permission, started_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                Text(agent.id),
                agent.position,
                agent.name,
                Text(&agent.role),
                text(agent.preset.as_ref()),
                agent.command,
                agent.session_id,
                text(agent.permission.as_ref()),
                now()
            ],
        )?;
        self.event(Some(agent.id), "agent.started", Some(&agent.name))
    }

    /// Records that an agent's process ended.
    ///
    /// # Errors
    /// [`StoreError::Database`].
    pub fn end_agent(&self, id: AgentId, exit_code: Option<i32>) -> Result<(), StoreError> {
        self.db().execute(
            "UPDATE agents SET ended_at = ?1, exit_code = ?2 WHERE id = ?3",
            params![now(), exit_code, Text(id)],
        )?;
        self.event(
            Some(id),
            "agent.exited",
            exit_code.map(|c| c.to_string()).as_deref(),
        )
    }

    /// Agents in team order (lead first).
    ///
    /// # Errors
    /// [`StoreError::Database`].
    pub fn agents(&self) -> Result<Vec<AgentRow>, StoreError> {
        let db = self.db();
        let mut statement = db.prepare(
            "SELECT id, position, name, role, preset, command, session_id, permission FROM agents ORDER BY position",
        )?;
        let agents = statement
            .query_map([], |r| {
                Ok(AgentRow {
                    id: get(r, 0)?,
                    position: r.get(1)?,
                    name: r.get(2)?,
                    role: get(r, 3)?,
                    preset: get_opt(r, 4)?,
                    command: r.get(5)?,
                    session_id: r.get(6)?,
                    permission: get_opt(r, 7)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(agents)
    }

    // MARK: Voice

    /// Moves a recording into `audio/`, hashing it on the way.
    ///
    /// # Errors
    /// [`StoreError`] on I/O or database failure.
    pub fn add_audio(
        &self,
        recording: &Path,
        mime: &str,
        duration_ms: Option<u64>,
    ) -> Result<AudioRow, StoreError> {
        let bytes = std::fs::read(recording).map_err(|e| StoreError::io(recording, e))?;
        let sha256 = hex::encode(Sha256::digest(&bytes));
        let id = new_id();
        let extension = recording
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("audio");
        let path = self.dir.join("audio").join(format!("{id}.{extension}"));
        // Rename when on the same volume; copy when not.
        if std::fs::rename(recording, &path).is_err() {
            std::fs::write(&path, &bytes).map_err(|e| StoreError::io(&path, e))?;
            let _ = std::fs::remove_file(recording);
        }
        let row = AudioRow {
            id,
            path: path.display().to_string(),
            duration_ms,
            sha256,
        };
        self.db().execute(
            "INSERT INTO audio (id, path, mime, duration_ms, sha256, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![row.id, row.path, mime, row.duration_ms.and_then(|ms| i64::try_from(ms).ok()), row.sha256, now()],
        )?;
        Ok(row)
    }

    /// # Errors
    /// [`StoreError::Database`].
    pub fn add_transcript(
        &self,
        audio_id: &str,
        provider: &str,
        language: Option<&str>,
        text: &str,
    ) -> Result<String, StoreError> {
        let id = new_id();
        self.db().execute(
            "INSERT INTO transcripts (id, audio_id, provider, language, text, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, audio_id, provider, language, text, now()],
        )?;
        Ok(id)
    }

    /// The transcript of a recording, if there is one.
    ///
    /// # Errors
    /// [`StoreError::Database`].
    pub fn transcript(&self, audio_id: &str) -> Result<Option<String>, StoreError> {
        Ok(self
            .db()
            .query_row(
                "SELECT text FROM transcripts WHERE audio_id = ?1 ORDER BY created_at DESC",
                [audio_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    // MARK: Notes and messages

    /// A brief answer (`question` set) or a free note.
    ///
    /// # Errors
    /// [`StoreError::Database`].
    pub fn add_note(
        &self,
        question: Option<&str>,
        text: &str,
        audio_id: Option<&str>,
    ) -> Result<String, StoreError> {
        let id = new_id();
        self.db().execute(
            "INSERT INTO notes (id, question, text, audio_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, question, text, audio_id, now()],
        )?;
        Ok(id)
    }

    /// # Errors
    /// [`StoreError::Database`].
    pub fn notes(&self) -> Result<Vec<NoteRow>, StoreError> {
        let db = self.db();
        let mut statement = db.prepare(
            "SELECT id, question, text, audio_id, created_at FROM notes ORDER BY created_at",
        )?;
        let notes = statement
            .query_map([], |r| {
                Ok(NoteRow {
                    id: r.get(0)?,
                    question: r.get(1)?,
                    text: r.get(2)?,
                    audio_id: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(notes)
    }

    /// Records something the user sent to agents.
    ///
    /// # Errors
    /// [`StoreError::Database`].
    pub fn add_message(
        &self,
        text: &str,
        recipients: &[AgentId],
        audio_id: Option<&str>,
    ) -> Result<String, StoreError> {
        let id = new_id();
        let mut db = self.db();
        let transaction = db.transaction()?;
        transaction.execute(
            "INSERT INTO messages (id, text, audio_id, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, text, audio_id, now()],
        )?;
        for agent in recipients {
            transaction.execute(
                "INSERT OR IGNORE INTO message_recipients (message_id, agent_id) VALUES (?1, ?2)",
                params![id, Text(agent)],
            )?;
        }
        transaction.commit()?;
        Ok(id)
    }

    /// # Errors
    /// [`StoreError::Database`].
    pub fn messages(&self) -> Result<Vec<MessageRow>, StoreError> {
        let db = self.db();
        let mut statement = db.prepare(
            "SELECT m.id, m.text, m.audio_id, m.created_at, group_concat(r.agent_id)
             FROM messages m LEFT JOIN message_recipients r ON r.message_id = m.id
             GROUP BY m.id ORDER BY m.created_at",
        )?;
        let messages = statement
            .query_map([], |r| {
                let recipients: Option<String> = r.get(4)?;
                let recipients = recipients
                    .iter()
                    .flat_map(|s| s.split(','))
                    .map(str::parse)
                    .collect::<Result<_, _>>()
                    .map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(4, Type::Text, Box::new(e))
                    })?;
                Ok(MessageRow {
                    id: r.get(0)?,
                    text: r.get(1)?,
                    audio_id: r.get(2)?,
                    created_at: r.get(3)?,
                    recipients,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(messages)
    }

    // MARK: Events

    /// Appends to the journal.
    ///
    /// # Errors
    /// [`StoreError::Database`].
    pub fn event(
        &self,
        agent_id: Option<AgentId>,
        kind: &str,
        detail: Option<&str>,
    ) -> Result<(), StoreError> {
        self.db().execute(
            "INSERT INTO events (at, agent_id, kind, detail) VALUES (?1, ?2, ?3, ?4)",
            params![now(), agent_id.map(Text), kind, detail],
        )?;
        Ok(())
    }

    /// # Errors
    /// [`StoreError::Database`].
    pub fn events(&self) -> Result<Vec<EventRow>, StoreError> {
        let db = self.db();
        let mut statement =
            db.prepare("SELECT seq, at, agent_id, kind, detail FROM events ORDER BY seq")?;
        let events = statement
            .query_map([], |r| {
                Ok(EventRow {
                    seq: r.get(0)?,
                    at: r.get(1)?,
                    agent_id: get_opt(r, 2)?,
                    kind: r.get(3)?,
                    detail: r.get(4)?,
                })
            })?
            .collect::<Result<_, _>>()?;
        Ok(events)
    }

    /// A panic mid-write leaves `SQLite` consistent (it rolls back), so
    /// poisoning is ignored.
    fn db(&self) -> MutexGuard<'_, Connection> {
        self.db.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use super::*;
    use crate::{Paths, Store};

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(Paths::under(dir.path())).unwrap();
        (dir, store)
    }

    fn new_work(name: &str) -> NewWork {
        NewWork::new(WorkspaceId::new(), name)
            .with_kind(Some(WorkKind::Fix))
            .with_template(Some("fix".into()))
            .with_folder(Some("/tmp/app".into()))
            .with_permission(Some(PermissionMode::Auto))
    }

    #[test]
    fn migrations_are_valid() {
        Migrations::from_slice(MIGRATIONS).validate().unwrap();
    }

    #[test]
    fn a_bad_stored_value_is_an_error_not_a_missing_row() {
        let (_dir, store) = store();
        let work = store.create_work(&new_work("Fix login")).unwrap();
        work.db()
            .execute("UPDATE work SET permission = 'yolo'", [])
            .unwrap();
        assert!(matches!(work.work(), Err(StoreError::Database(_))));
    }

    #[test]
    fn a_work_keeps_its_team_notes_voice_and_messages() {
        let (_dir, store) = store();
        let work = store.create_work(&new_work("Fix login")).unwrap();
        assert!(
            work.dir()
                .ends_with(format!("{}-fix-login", work.work().unwrap().id))
        );

        assert_eq!(work.work().unwrap().permission, Some(PermissionMode::Auto));
        let (a1, a2) = (AgentId::new(), AgentId::new());
        let lead = AgentRow::new(a1, 0, "claude", AgentRole::Implementer)
            .started_with(
                Some("claude".into()),
                Some("claude --session-id s1".into()),
                Some("s1".into()),
            )
            .with_permission(Some(PermissionMode::Auto));
        work.add_agent(&lead).unwrap();
        work.end_agent(a1, Some(0)).unwrap();
        assert_eq!(work.agents().unwrap(), vec![lead]);

        let recording = store.paths().recordings_dir.join("note.m4a");
        std::fs::write(&recording, b"fake audio").unwrap();
        let audio = work.add_audio(&recording, "audio/mp4", Some(1200)).unwrap();
        assert!(!recording.exists(), "moved into the work");
        assert!(Path::new(&audio.path).exists());
        work.add_transcript(&audio.id, "apple", Some("en-US"), "it loops on Safari")
            .unwrap();
        assert_eq!(
            work.transcript(&audio.id).unwrap().as_deref(),
            Some("it loops on Safari")
        );

        work.add_note(Some("What's broken?"), "login loops", None)
            .unwrap();
        work.add_note(None, "it loops on Safari", Some(&audio.id))
            .unwrap();
        assert_eq!(work.notes().unwrap().len(), 2);

        work.add_message("@all summarise", &[a1, a2], None).unwrap();
        let messages = work.messages().unwrap();
        assert_eq!(messages[0].recipients, [a1, a2]);

        let kinds: Vec<_> = work.events().unwrap().into_iter().map(|e| e.kind).collect();
        assert_eq!(kinds, ["work.created", "agent.started", "agent.exited"]);

        let brief = work.write_brief("# Fix login\\n").unwrap();
        assert!(brief.exists());
    }

    #[test]
    fn the_index_lists_recent_work_and_reopens_it() {
        let (_dir, store) = store();
        let first = store.create_work(&new_work("one")).unwrap().work().unwrap();
        let second = store.create_work(&new_work("two")).unwrap().work().unwrap();

        let recent = store.index().recent(10).unwrap();
        assert_eq!(
            recent.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(),
            ["two", "one"]
        );
        assert_eq!(store.index().frecent_folders(5).unwrap(), ["/tmp/app"]);

        let reopened = store.open_work(first.id).unwrap();
        assert_eq!(reopened.work().unwrap().name, "one");
        assert_eq!(
            store.index().recent(1).unwrap()[0].id,
            first.id,
            "reopening moves it to the top"
        );
        assert!(matches!(
            store.open_work(WorkspaceId::new()),
            Err(StoreError::UnknownWork(_))
        ));

        store
            .discard_work(store.open_work(second.id).unwrap())
            .unwrap();
        assert_eq!(store.index().recent(10).unwrap().len(), 1);
        assert!(store.index().get(second.id).unwrap().is_none());
    }

    #[test]
    fn folders_used_more_rank_first() {
        let (_dir, store) = store();
        let in_folder = |name: &str, folder: &str| {
            NewWork::new(WorkspaceId::new(), name).with_folder(Some(folder.into()))
        };
        store.create_work(&in_folder("one", "/code/daily")).unwrap();
        store.create_work(&in_folder("two", "/code/daily")).unwrap();
        store
            .create_work(&in_folder("three", "/code/once"))
            .unwrap();
        assert_eq!(
            store.index().frecent_folders(5).unwrap(),
            ["/code/daily", "/code/once"]
        );
    }
}
