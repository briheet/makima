use std::convert::Infallible;
use std::fmt;
use std::str::FromStr;

use crate::{AgentId, PermissionMode, PresetId};

/// What an agent is responsible for inside its workspace.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum AgentRole {
    Planner,
    Implementer,
    Reviewer,
    Tester,
    Researcher,
    /// A plain interactive shell.
    Shell,
    Custom(String),
}

impl AgentRole {
    /// The role as a lowercase word, for summaries, prompts and storage.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Planner => "planner",
            Self::Implementer => "implementer",
            Self::Reviewer => "reviewer",
            Self::Tester => "tester",
            Self::Researcher => "researcher",
            Self::Shell => "shell",
            Self::Custom(name) => name,
        }
    }
}

impl fmt::Display for AgentRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.as_str())
    }
}

/// The inverse of [`AgentRole::as_str`]. Any word is a role: unknown ones
/// become [`AgentRole::Custom`].
impl FromStr for AgentRole {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "planner" => Self::Planner,
            "implementer" => Self::Implementer,
            "reviewer" => Self::Reviewer,
            "tester" => Self::Tester,
            "researcher" => Self::Researcher,
            "shell" => Self::Shell,
            other => Self::Custom(other.to_owned()),
        })
    }
}

impl From<&str> for AgentRole {
    fn from(word: &str) -> Self {
        match word.parse() {
            Ok(role) => role,
            Err(never) => match never {},
        }
    }
}

/// Where an agent is in its lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum AgentStatus {
    Starting,
    Idle,
    Working,
    InputRequired {
        prompt: String,
    },
    Completed,
    Failed {
        reason: String,
    },
    /// The process could not be proved alive after a restart.
    Interrupted,
}

/// The status as stored in a work's journal: `working`, `input-required:
/// <prompt>`, `failed: <reason>`, ….
impl fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Starting => f.write_str("starting"),
            Self::Idle => f.write_str("idle"),
            Self::Working => f.write_str("working"),
            Self::InputRequired { prompt } => write!(f, "input-required: {prompt}"),
            Self::Completed => f.write_str("completed"),
            Self::Failed { reason } => write!(f, "failed: {reason}"),
            Self::Interrupted => f.write_str("interrupted"),
        }
    }
}

impl AgentStatus {
    /// Whether the agent has stopped for good.
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed { .. })
    }

    /// The status for a process that exited. A `None` code means the exit
    /// status is unknown (e.g. the user closed an interactive shell), which
    /// counts as a normal finish.
    #[must_use]
    pub fn from_exit(code: Option<i32>) -> Self {
        match code {
            None | Some(0) => Self::Completed,
            Some(code) => Self::Failed {
                reason: format!("exited with code {code}"),
            },
        }
    }
}

/// A long-running worker in a workspace, backed by a terminal process.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Agent {
    id: AgentId,
    name: String,
    role: AgentRole,
    command: Option<String>,
    status: AgentStatus,
    /// The preset it was started from, if any.
    #[serde(default)]
    preset: Option<PresetId>,
    /// The program's own session id (e.g. Claude Code's), for resuming.
    #[serde(default)]
    session_id: Option<String>,
    /// The permission mode it was started in, when its program has one.
    #[serde(default)]
    permission: Option<PermissionMode>,
}

impl Agent {
    /// A new agent in [`AgentStatus::Starting`]. `command` is `None` for the
    /// user's default shell.
    #[must_use]
    pub fn new(name: impl Into<String>, role: AgentRole, command: Option<String>) -> Self {
        Self {
            id: AgentId::new(),
            name: name.into(),
            role,
            command,
            status: AgentStatus::Starting,
            preset: None,
            session_id: None,
            permission: None,
        }
    }

    /// Records the preset and program session this agent was started with.
    #[must_use]
    pub fn with_origin(mut self, preset: Option<PresetId>, session_id: Option<String>) -> Self {
        self.preset = preset;
        self.session_id = session_id;
        self
    }

    /// Records the permission mode it was started in.
    #[must_use]
    pub fn with_permission(mut self, permission: Option<PermissionMode>) -> Self {
        self.permission = permission;
        self
    }

    #[must_use]
    pub fn permission(&self) -> Option<PermissionMode> {
        self.permission
    }

    #[must_use]
    pub fn preset(&self) -> Option<&PresetId> {
        self.preset.as_ref()
    }

    #[must_use]
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    #[must_use]
    pub fn id(&self) -> AgentId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn role(&self) -> &AgentRole {
        &self.role
    }

    #[must_use]
    pub fn command(&self) -> Option<&str> {
        self.command.as_deref()
    }

    #[must_use]
    pub fn status(&self) -> &AgentStatus {
        &self.status
    }

    pub(crate) fn set_status(&mut self, status: AgentStatus) {
        self.status = status;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_round_trip_through_their_words() {
        for role in [
            AgentRole::Planner,
            AgentRole::Implementer,
            AgentRole::Reviewer,
            AgentRole::Tester,
            AgentRole::Researcher,
            AgentRole::Shell,
            AgentRole::Custom("security".into()),
        ] {
            assert_eq!(AgentRole::from(role.as_str()), role);
        }
    }
}
