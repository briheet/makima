use std::fmt;
use std::sync::Arc;

use crate::AgentRole;
use crate::adapter::{AgentAdapter, ClaudeCode, Codex, Shell};
use crate::permission::PermissionMode;

/// Identifies an [`AgentPreset`]: a stable word like `claude`, used in the
/// config file, templates, saved work and across the FFI.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct PresetId(String);

impl PresetId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PresetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(&self.0)
    }
}

impl std::str::FromStr for PresetId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s))
    }
}

impl From<&str> for PresetId {
    fn from(id: &str) -> Self {
        Self::new(id)
    }
}

impl From<String> for PresetId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl PartialEq<str> for PresetId {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for PresetId {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

/// A ready-made kind of agent the user can start with one click: a name
/// for an [`AgentAdapter`]. Built-in presets can be extended or replaced
/// from the config file.
#[derive(Debug, Clone)]
pub struct AgentPreset {
    id: PresetId,
    label: String,
    base_name: String,
    role: AgentRole,
    adapter: Arc<dyn AgentAdapter>,
}

/// How to start one agent from a preset. Build with [`Launch::new`] and
/// the `with_*` methods.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Launch {
    /// A session id to pin (new session) or resume.
    pub session_id: Option<String>,
    /// Resume `session_id` instead of starting it.
    pub resume: bool,
    /// The initial prompt, for programs that accept one.
    pub prompt: Option<String>,
    /// How much the agent may do without asking; `None` is the program's
    /// own default.
    pub permission: Option<PermissionMode>,
}

impl Launch {
    /// No session and no prompt: the program as configured.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pins a new session with `id`.
    #[must_use]
    pub fn with_session(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self.resume = false;
        self
    }

    /// Resumes the session `id` (if any) instead of starting fresh.
    #[must_use]
    pub fn resuming(mut self, id: Option<String>) -> Self {
        self.resume = id.is_some();
        self.session_id = id;
        self
    }

    #[must_use]
    pub fn with_prompt(mut self, prompt: Option<String>) -> Self {
        self.prompt = prompt;
        self
    }

    #[must_use]
    pub fn with_permission(mut self, permission: Option<PermissionMode>) -> Self {
        self.permission = permission;
        self
    }
}

impl AgentPreset {
    /// A preset whose agents are named after the program `adapter` runs.
    #[must_use]
    pub fn new(
        id: impl Into<PresetId>,
        label: impl Into<String>,
        adapter: Arc<dyn AgentAdapter>,
    ) -> Self {
        let command = adapter.command();
        let base_name = command
            .and_then(|c| c.split_whitespace().next())
            .unwrap_or("shell")
            .to_owned();
        let role = if command.is_some() {
            AgentRole::Implementer
        } else {
            AgentRole::Shell
        };
        Self {
            id: id.into(),
            label: label.into(),
            base_name,
            role,
            adapter,
        }
    }

    /// Stable identifier, used across the FFI and in the config file.
    #[must_use]
    pub fn id(&self) -> &PresetId {
        &self.id
    }

    /// Human-readable name, e.g. "Claude Code".
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Prefix for agent names: "claude", "claude 2", …
    #[must_use]
    pub fn base_name(&self) -> &str {
        &self.base_name
    }

    /// Program to run; `None` runs the user's login shell.
    #[must_use]
    pub fn command(&self) -> Option<&str> {
        self.adapter.command()
    }

    #[must_use]
    pub fn role(&self) -> &AgentRole {
        &self.role
    }

    /// What Makima knows about the program.
    #[must_use]
    pub fn adapter(&self) -> &dyn AgentAdapter {
        &*self.adapter
    }

    /// The shell command line that starts this preset, or `None` for the
    /// login shell. Options the program doesn't support are left out.
    #[must_use]
    pub fn command_line(&self, launch: &Launch) -> Option<String> {
        self.adapter.command_line(launch)
    }
}

/// `text` as one POSIX shell word.
#[must_use]
pub fn shell_quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@".contains(c))
    {
        return text.to_owned();
    }
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// The preset new work's lead runs unless the config or template says
/// otherwise.
pub const DEFAULT_LEAD: &str = "claude";

/// The presets Makima ships with, in display order.
#[must_use]
pub fn builtin_presets() -> Vec<AgentPreset> {
    vec![
        AgentPreset::new("shell", "Shell", Arc::new(Shell)),
        AgentPreset::new(DEFAULT_LEAD, "Claude Code", Arc::new(ClaudeCode::default())),
        AgentPreset::new("codex", "Codex", Arc::new(Codex::default())),
    ]
}

/// `base` with `overrides` applied: a preset with an existing id replaces it
/// in place, a new id is appended.
#[must_use]
pub fn merge_presets(mut base: Vec<AgentPreset>, overrides: Vec<AgentPreset>) -> Vec<AgentPreset> {
    for preset in overrides {
        match base.iter_mut().find(|p| p.id == preset.id) {
            Some(existing) => *existing = preset,
            None => base.push(preset),
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::Configured;

    #[test]
    fn overrides_replace_by_id_and_append_new_ones() {
        let merged = merge_presets(
            builtin_presets(),
            vec![
                AgentPreset::new(
                    "claude",
                    "Claude (yolo)",
                    Arc::new(ClaudeCode::new("claude --dangerously-skip-permissions")),
                ),
                AgentPreset::new(
                    "aider",
                    "Aider",
                    Arc::new(Configured::new(Some("aider".into()))),
                ),
            ],
        );
        let ids: Vec<_> = merged.iter().map(|p| p.id().as_str()).collect();
        assert_eq!(ids, ["shell", "claude", "codex", "aider"]);
        assert_eq!(merged[1].base_name(), "claude");
        assert_eq!(merged[1].label(), "Claude (yolo)");
    }

    #[test]
    fn presets_delegate_to_their_adapter() {
        let presets = builtin_presets();
        let launch = Launch::new()
            .with_session("s1")
            .with_prompt(Some("go".into()));
        assert_eq!(
            presets[1].command_line(&launch).as_deref(),
            Some("claude --session-id s1 go")
        );
        assert_eq!(presets[0].command_line(&launch), None);
        assert_eq!(presets[0].role(), &AgentRole::Shell);
        assert_eq!(presets[2].role(), &AgentRole::Implementer);
    }
}
