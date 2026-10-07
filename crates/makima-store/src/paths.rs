use std::path::{Path, PathBuf};

use etcetera::app_strategy::{AppStrategy, AppStrategyArgs};

use crate::StoreError;

/// Every place Makima reads or writes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Paths {
    /// `~/.config/makima/config.toml`.
    pub config_file: PathBuf,
    /// `~/.local/share/makima`, or `[storage] root`.
    pub data_dir: PathBuf,
    /// `<data>/work`: one folder per work.
    pub works_dir: PathBuf,
    /// `<data>/index.db`.
    pub index_db: PathBuf,
    /// `~/.local/state/makima`.
    pub state_dir: PathBuf,
    /// `~/.cache/makima`.
    pub cache_dir: PathBuf,
    /// `<cache>/recordings`: voice notes recorded before their work exists.
    pub recordings_dir: PathBuf,
}

impl Paths {
    /// XDG locations (on macOS too), with `storage_root` replacing the data
    /// directory when set. `~` in `storage_root` is expanded.
    ///
    /// `MAKIMA_HOME`, when set, puts everything under one directory instead
    /// (see [`Self::under`]): development runs keep their databases away
    /// from real work, and `just clean-dev` removes them.
    ///
    /// # Errors
    /// [`StoreError::NoHome`] if the home directory can't be found.
    pub fn resolve(storage_root: Option<&str>) -> Result<Self, StoreError> {
        if let Some(home) = std::env::var_os("MAKIMA_HOME").filter(|h| !h.is_empty()) {
            return Ok(Self::under(&PathBuf::from(home)));
        }
        let strategy = etcetera::choose_app_strategy(AppStrategyArgs {
            top_level_domain: "io".into(),
            author: "makima".into(),
            app_name: "makima".into(),
        })
        .map_err(|_| StoreError::NoHome)?;
        let data_dir = match storage_root {
            Some(root) => expand_tilde(root)?,
            None => strategy.data_dir(),
        };
        let cache_dir = strategy.cache_dir();
        Ok(Self {
            config_file: strategy.config_dir().join("config.toml"),
            works_dir: data_dir.join("work"),
            index_db: data_dir.join("index.db"),
            state_dir: strategy
                .state_dir()
                .unwrap_or_else(|| data_dir.join("state")),
            recordings_dir: cache_dir.join("recordings"),
            data_dir,
            cache_dir,
        })
    }

    /// Everything under one directory, for tests and portable setups.
    #[must_use]
    pub fn under(root: &Path) -> Self {
        Self {
            config_file: root.join("config/config.toml"),
            data_dir: root.join("data"),
            works_dir: root.join("data/work"),
            index_db: root.join("data/index.db"),
            state_dir: root.join("state"),
            cache_dir: root.join("cache"),
            recordings_dir: root.join("cache/recordings"),
        }
    }
}

/// `path` with a leading `~` replaced by the home directory.
///
/// # Errors
/// [`StoreError::NoHome`] if the path needs it and it can't be found.
pub fn expand_tilde(path: &str) -> Result<PathBuf, StoreError> {
    match path.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            let home = etcetera::home_dir().map_err(|_| StoreError::NoHome)?;
            Ok(home.join(rest.trim_start_matches('/')))
        }
        _ => Ok(PathBuf::from(path)),
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use super::*;

    #[test]
    fn xdg_layout_and_overrides() {
        let paths = Paths::resolve(None).unwrap();
        assert!(paths.config_file.ends_with("makima/config.toml"));
        assert!(paths.works_dir.ends_with("makima/work"));
        let custom = Paths::resolve(Some("/tmp/vault")).unwrap();
        assert_eq!(custom.index_db, PathBuf::from("/tmp/vault/index.db"));
    }

    #[test]
    fn tilde_expands_only_at_the_start() {
        let home = etcetera::home_dir().unwrap();
        assert_eq!(expand_tilde("~/code").unwrap(), home.join("code"));
        assert_eq!(expand_tilde("~").unwrap(), home);
        assert_eq!(expand_tilde("/a/~b").unwrap(), PathBuf::from("/a/~b"));
        assert_eq!(expand_tilde("~other").unwrap(), PathBuf::from("~other"));
    }
}
