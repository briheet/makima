//! Where Makima keeps work.
//!
//! ```text
//! ~/.config/makima/config.toml      settings            (paths::Paths)
//! ~/.local/share/makima/            durable data, or [storage] root
//!   index.db                        every work, for recents  (IndexDb)
//!   work/<id>-<slug>/
//!     work.db                       this work's database (WorkDb)
//!     brief.md                      what the agents read
//!     audio/<id>.m4a                voice recordings
//! ~/.local/state/makima/            logs and window state
//! ~/.cache/makima/                  regenerable files only (recordings in progress)
//! ```
//!
//! Paths follow XDG on macOS too, as CLI tools do (`gh`, `git`), via
//! `etcetera`. Each work is a self-contained folder: archive, move or
//! delete it as one unit. Databases use WAL and versioned migrations.

mod discover;
mod error;
mod index;
mod paths;
mod sessions;
mod sql;
mod work;

pub use discover::{RepoScan, discover_repos};
pub use error::StoreError;
pub use index::{IndexDb, WorkSummary};
use makima_core::WorkspaceId;
pub use paths::{Paths, expand_tilde};
pub use sessions::Sessions;

pub use work::{AgentRow, AudioRow, EventRow, MessageRow, NewWork, NoteRow, WorkDb, WorkRow};

/// The current time as RFC 3339 (UTC), as stored everywhere.
#[must_use]
pub fn now() -> String {
    jiff::Timestamp::now().to_string()
}

/// A new time-ordered id, for rows that aren't domain objects (notes,
/// audio, messages).
#[must_use]
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Opens (or creates) the store under `paths`.
///
/// # Errors
/// [`StoreError`] if the directories or index can't be created.
pub fn open(paths: Paths) -> Result<Store, StoreError> {
    Store::open(paths)
}

/// The store: paths, the index, and creating or opening works.
#[derive(Debug)]
pub struct Store {
    paths: Paths,
    index: IndexDb,
}

impl Store {
    /// # Errors
    /// [`StoreError`] if the directories or index can't be created.
    pub fn open(paths: Paths) -> Result<Self, StoreError> {
        for dir in [&paths.works_dir, &paths.recordings_dir] {
            std::fs::create_dir_all(dir).map_err(|e| StoreError::io(dir, e))?;
        }
        let index = IndexDb::open(&paths.index_db)?;
        Ok(Self { paths, index })
    }

    #[must_use]
    pub fn paths(&self) -> &Paths {
        &self.paths
    }

    #[must_use]
    pub fn index(&self) -> &IndexDb {
        &self.index
    }

    /// Removes a work created by [`Self::create_work`] whose start failed:
    /// its folder and its index entry.
    ///
    /// # Errors
    /// [`StoreError`] on I/O or database failure.
    pub fn discard_work(&self, work: WorkDb) -> Result<(), StoreError> {
        let id = work.work()?.id;
        let dir = work.dir().to_owned();
        drop(work);
        self.index.remove(id)?;
        std::fs::remove_dir_all(&dir).map_err(|e| StoreError::io(&dir, e))
    }

    /// Creates a work's folder and database and lists it in the index.
    ///
    /// # Errors
    /// [`StoreError`] on I/O or database failure.
    pub fn create_work(&self, new: &NewWork) -> Result<WorkDb, StoreError> {
        let dir = self
            .paths
            .works_dir
            .join(format!("{}-{}", new.id, slug(&new.name)));
        let work = WorkDb::create(&dir, new)?;
        self.index.register(&WorkSummary {
            id: new.id,
            name: new.name.clone(),
            kind: new.kind,
            folder: new.folder.clone(),
            dir: dir.display().to_string(),
            opened_at: now(),
        })?;
        Ok(work)
    }

    /// Opens a work listed in the index and marks it as just opened.
    ///
    /// # Errors
    /// [`StoreError::UnknownWork`] if it isn't in the index, or on I/O or
    /// database failure.
    pub fn open_work(&self, id: WorkspaceId) -> Result<WorkDb, StoreError> {
        let summary = self.index.get(id)?.ok_or(StoreError::UnknownWork(id))?;
        let work = WorkDb::open(std::path::Path::new(&summary.dir))?;
        self.index.touch(id)?;
        Ok(work)
    }
}

/// A folder-name-safe version of `name`: lowercase, dashes, at most 40
/// characters.
#[must_use]
pub fn slug(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug: String = slug.trim_end_matches('-').chars().take(40).collect();
    if slug.is_empty() { "work".into() } else { slug }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_folder_safe() {
        assert_eq!(slug("Fix flaky CI!"), "fix-flaky-ci");
        assert_eq!(slug("  ✓✓  "), "work");
    }
}
