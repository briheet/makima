use std::path::{Path, PathBuf};

use makima_core::WorkspaceId;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StoreError {
    #[error("{}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("database")]
    Database(#[from] rusqlite::Error),
    #[error("database migration")]
    Migration(#[from] rusqlite_migration::Error),
    #[error("can't find the home directory")]
    NoHome,
    #[error("no work with id {0}")]
    UnknownWork(WorkspaceId),
}

impl StoreError {
    pub(crate) fn io(path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            source,
        }
    }
}
