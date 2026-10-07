use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusqlite::{Connection, OptionalExtension, params};
use rusqlite_migration::{M, Migrations};

use makima_core::{WorkKind, WorkspaceId};

use crate::sql::{Text, get, get_opt, text};
use crate::{StoreError, now};

/// One work, as the welcome screen lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorkSummary {
    pub id: WorkspaceId,
    pub name: String,
    pub kind: Option<WorkKind>,
    /// Where the agents work.
    pub folder: Option<String>,
    /// The work's own folder (database, brief, audio).
    pub dir: String,
    pub opened_at: String,
}

/// `index.db`: every work, most recently opened first.
#[derive(Debug)]
pub struct IndexDb(Mutex<Connection>);

const MIGRATIONS: &[M<'static>] = &[M::up(
    "CREATE TABLE works (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        kind TEXT,
        folder TEXT,
        dir TEXT NOT NULL,
        opened_at TEXT NOT NULL
    );
    CREATE INDEX works_by_opened ON works(opened_at DESC);",
)];

impl IndexDb {
    /// # Errors
    /// [`StoreError`] if the database can't be opened or migrated.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let mut connection = crate::work::connect(path)?;
        Migrations::from_slice(MIGRATIONS).to_latest(&mut connection)?;
        Ok(Self(Mutex::new(connection)))
    }

    /// # Errors
    /// [`StoreError::Database`].
    pub fn register(&self, work: &WorkSummary) -> Result<(), StoreError> {
        self.db().execute(
            "INSERT OR REPLACE INTO works (id, name, kind, folder, dir, opened_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                Text(work.id),
                work.name,
                text(work.kind.as_ref()),
                work.folder,
                work.dir,
                work.opened_at
            ],
        )?;
        Ok(())
    }

    /// Marks a work as just opened.
    ///
    /// # Errors
    /// [`StoreError::Database`].
    pub fn touch(&self, id: WorkspaceId) -> Result<(), StoreError> {
        self.db().execute(
            "UPDATE works SET opened_at = ?1 WHERE id = ?2",
            params![now(), Text(id)],
        )?;
        Ok(())
    }

    /// # Errors
    /// [`StoreError::Database`].
    pub fn remove(&self, id: WorkspaceId) -> Result<(), StoreError> {
        self.db()
            .execute("DELETE FROM works WHERE id = ?1", [Text(id)])?;
        Ok(())
    }

    /// # Errors
    /// [`StoreError::Database`].
    pub fn get(&self, id: WorkspaceId) -> Result<Option<WorkSummary>, StoreError> {
        Ok(self
            .db()
            .query_row(
                "SELECT id, name, kind, folder, dir, opened_at FROM works WHERE id = ?1",
                [Text(id)],
                row,
            )
            .optional()?)
    }

    /// The `limit` most recently opened works whose folder still exists.
    ///
    /// # Errors
    /// [`StoreError::Database`], including for a row that doesn't parse.
    pub fn recent(&self, limit: usize) -> Result<Vec<WorkSummary>, StoreError> {
        let db = self.db();
        let mut statement = db.prepare(
            "SELECT id, name, kind, folder, dir, opened_at FROM works ORDER BY opened_at DESC",
        )?;
        let mut works = Vec::new();
        for work in statement.query_map([], row)? {
            let work = work?;
            if Path::new(&work.dir).join("work.db").exists() {
                works.push(work);
            }
            if works.len() == limit {
                break;
            }
        }
        Ok(works)
    }

    /// Folders agents have worked in, most used first: each work counts
    /// for its folder, weighted by how recently it was opened (like
    /// Firefox's frecency), so a folder used daily beats one used once.
    ///
    /// # Errors
    /// [`StoreError::Database`].
    pub fn frecent_folders(&self, limit: usize) -> Result<Vec<String>, StoreError> {
        let db = self.db();
        let mut statement =
            db.prepare("SELECT folder, opened_at FROM works WHERE folder IS NOT NULL")?;
        let now = jiff::Timestamp::now();
        let mut scores: Vec<(String, u32, String)> = Vec::new();
        for row in
            statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        {
            let (folder, opened_at) = row?;
            let weight = opened_at
                .parse::<jiff::Timestamp>()
                .map_or(1, |at| recency_weight(now.duration_since(at)));
            match scores.iter_mut().find(|(f, _, _)| *f == folder) {
                Some((_, score, latest)) => {
                    *score += weight;
                    if opened_at > *latest {
                        *latest = opened_at;
                    }
                }
                None => scores.push((folder, weight, opened_at)),
            }
        }
        // Highest score first; ties go to the most recently opened.
        scores.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.2.cmp(&a.2)));
        Ok(scores.into_iter().take(limit).map(|(f, _, _)| f).collect())
    }

    fn db(&self) -> MutexGuard<'_, Connection> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// How much one use counts, by its age.
fn recency_weight(age: jiff::SignedDuration) -> u32 {
    const DAY: i64 = 24 * 60 * 60;
    match age.as_secs() / DAY {
        ..4 => 100,
        4..14 => 70,
        14..31 => 50,
        31..90 => 30,
        _ => 10,
    }
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<WorkSummary> {
    Ok(WorkSummary {
        id: get(r, 0)?,
        name: r.get(1)?,
        kind: get_opt(r, 2)?,
        folder: r.get(3)?,
        dir: r.get(4)?,
        opened_at: r.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_use_counts_more() {
        let day = jiff::SignedDuration::from_hours(24);
        assert_eq!(recency_weight(day), 100);
        assert_eq!(recency_weight(day * 20), 50);
        assert_eq!(recency_weight(day * 400), 10);
    }
}
