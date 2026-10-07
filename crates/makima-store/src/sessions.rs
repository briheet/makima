//! What agent programs have saved, so reopened work only resumes sessions
//! that exist. Which program saves where is the adapter's
//! [`SessionStore`]; looking is done here, keeping the core free of I/O.

use std::path::{Path, PathBuf};

use makima_core::SessionStore;

/// Where each program keeps its sessions on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sessions {
    /// Claude Code's `projects` folder.
    claude_projects: Option<PathBuf>,
}

impl Sessions {
    /// The programs' usual places: Claude Code's is
    /// `$CLAUDE_CONFIG_DIR/projects`, or `~/.claude/projects`.
    #[must_use]
    pub fn find() -> Self {
        let claude = std::env::var_os("CLAUDE_CONFIG_DIR")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
            .or_else(|| etcetera::home_dir().ok().map(|h| h.join(".claude")));
        Self {
            claude_projects: claude.map(|c| c.join("projects")),
        }
    }

    /// Claude Code's sessions under `claude_projects` (tests).
    #[must_use]
    pub fn at(claude_projects: Option<PathBuf>) -> Self {
        Self { claude_projects }
    }

    /// Whether session `id` was saved where `store` says. Assumed saved
    /// when Makima can't look, so resuming is never skipped by mistake.
    #[must_use]
    pub fn saved(&self, store: SessionStore, id: &str) -> bool {
        match store {
            SessionStore::ClaudeCode => self
                .claude_projects
                .as_deref()
                .is_none_or(|projects| claude_session_saved(projects, id)),
            _ => true,
        }
    }
}

/// Claude writes a session to `<projects>/<folder>/<id>.jsonl` once it gets
/// its first message; an agent that never got one has nothing to resume.
/// Folder names are Claude's own encoding of the path, so every one is
/// checked rather than guessed.
fn claude_session_saved(projects: &Path, id: &str) -> bool {
    let file = format!("{id}.jsonl");
    std::fs::read_dir(projects).is_ok_and(|folders| {
        folders
            .filter_map(Result::ok)
            .any(|folder| folder.path().join(&file).is_file())
    })
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use super::*;

    #[test]
    fn finds_saved_claude_sessions_in_any_project() {
        let projects = tempfile::tempdir().unwrap();
        let folder = projects.path().join("-Users-me-code-app");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("abc.jsonl"), "{}").unwrap();
        let sessions = Sessions::at(Some(projects.path().to_owned()));
        assert!(sessions.saved(SessionStore::ClaudeCode, "abc"));
        assert!(!sessions.saved(SessionStore::ClaudeCode, "never-sent"));
        let elsewhere = Sessions::at(Some("/nonexistent".into()));
        assert!(!elsewhere.saved(SessionStore::ClaudeCode, "abc"));
        assert!(
            Sessions::at(None).saved(SessionStore::ClaudeCode, "abc"),
            "can't look, so assumed saved"
        );
    }
}
