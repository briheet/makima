//! Records and enums that cross the FFI boundary, and their conversions
//! from the core's domain types. IDs cross as typed custom types (see
//! `lib.rs`).

use makima_core::{self as core, AgentId, PresetId, WorkspaceId};

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum AgentRole {
    Planner,
    Implementer,
    Reviewer,
    Tester,
    Researcher,
    Shell,
    Custom { name: String },
}

/// What a piece of work is (see `makima_core::WorkKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum WorkKind {
    Feat,
    Fix,
    Refactor,
    Research,
    Review,
    Docs,
}

/// How much an agent may do without asking (see
/// `makima_core::PermissionMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PermissionMode {
    Manual,
    AcceptEdits,
    Plan,
    Auto,
    Bypass,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum AgentStatus {
    Starting,
    Idle,
    Working,
    InputRequired { prompt: String },
    Completed,
    Failed { reason: String },
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AgentInfo {
    pub id: AgentId,
    /// The program's own session id (Claude Code's), when pinned.
    pub session_id: Option<String>,
    pub name: String,
    pub role: AgentRole,
    /// `None` runs the user's default shell.
    pub command: Option<String>,
    pub status: AgentStatus,
    /// The mode it was started in, when its program has one.
    pub permission: Option<PermissionMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct WorkspaceInfo {
    pub id: WorkspaceId,
    /// Where the agents work.
    pub folder: Option<String>,
    pub name: String,
    pub agents: Vec<AgentInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AgentPresetInfo {
    pub id: PresetId,
    pub label: String,
    /// `None` runs the user's default shell.
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum CoreEvent {
    WorkspaceCreated {
        workspace: WorkspaceInfo,
    },
    AgentAdded {
        workspace_id: WorkspaceId,
        agent: AgentInfo,
    },
    AgentUpdated {
        agent: AgentInfo,
    },
    AgentRemoved {
        agent_id: AgentId,
    },
}

impl From<AgentRole> for core::AgentRole {
    fn from(role: AgentRole) -> Self {
        match role {
            AgentRole::Planner => Self::Planner,
            AgentRole::Implementer => Self::Implementer,
            AgentRole::Reviewer => Self::Reviewer,
            AgentRole::Tester => Self::Tester,
            AgentRole::Researcher => Self::Researcher,
            AgentRole::Shell => Self::Shell,
            AgentRole::Custom { name } => Self::Custom(name),
        }
    }
}

impl From<&core::AgentRole> for AgentRole {
    fn from(role: &core::AgentRole) -> Self {
        match role {
            core::AgentRole::Planner => Self::Planner,
            core::AgentRole::Implementer => Self::Implementer,
            core::AgentRole::Reviewer => Self::Reviewer,
            core::AgentRole::Tester => Self::Tester,
            core::AgentRole::Researcher => Self::Researcher,
            core::AgentRole::Shell => Self::Shell,
            core::AgentRole::Custom(name) => Self::Custom { name: name.clone() },
            // `core::AgentRole` is non_exhaustive: a role added later
            // crosses by its word.
            _ => Self::Custom {
                name: role.to_string(),
            },
        }
    }
}

impl From<AgentStatus> for core::AgentStatus {
    fn from(status: AgentStatus) -> Self {
        match status {
            AgentStatus::Starting => Self::Starting,
            AgentStatus::Idle => Self::Idle,
            AgentStatus::Working => Self::Working,
            AgentStatus::InputRequired { prompt } => Self::InputRequired { prompt },
            AgentStatus::Completed => Self::Completed,
            AgentStatus::Failed { reason } => Self::Failed { reason },
            AgentStatus::Interrupted => Self::Interrupted,
        }
    }
}

impl From<&core::AgentStatus> for AgentStatus {
    fn from(status: &core::AgentStatus) -> Self {
        match status {
            core::AgentStatus::Starting => Self::Starting,
            core::AgentStatus::Idle => Self::Idle,
            core::AgentStatus::Working => Self::Working,
            core::AgentStatus::InputRequired { prompt } => Self::InputRequired {
                prompt: prompt.clone(),
            },
            core::AgentStatus::Completed => Self::Completed,
            core::AgentStatus::Failed { reason } => Self::Failed {
                reason: reason.clone(),
            },
            // Interrupted, plus any status added to the core later.
            _ => Self::Interrupted,
        }
    }
}

impl From<&core::Agent> for AgentInfo {
    fn from(agent: &core::Agent) -> Self {
        Self {
            id: agent.id(),
            session_id: agent.session_id().map(str::to_owned),
            name: agent.name().to_owned(),
            role: agent.role().into(),
            command: agent.command().map(str::to_owned),
            status: agent.status().into(),
            permission: agent
                .permission()
                .and_then(|m| PermissionMode::try_from(m).ok()),
        }
    }
}

impl From<&core::Workspace> for WorkspaceInfo {
    fn from(workspace: &core::Workspace) -> Self {
        Self {
            id: workspace.id(),
            folder: workspace.folder().map(str::to_owned),
            name: workspace.name().as_str().to_owned(),
            agents: workspace.agents().iter().map(Into::into).collect(),
        }
    }
}

impl From<&core::AgentPreset> for AgentPresetInfo {
    fn from(preset: &core::AgentPreset) -> Self {
        Self {
            id: preset.id().clone(),
            label: preset.label().to_owned(),
            command: preset.command().map(str::to_owned),
        }
    }
}

impl From<WorkKind> for core::WorkKind {
    fn from(kind: WorkKind) -> Self {
        match kind {
            WorkKind::Feat => Self::Feat,
            WorkKind::Fix => Self::Fix,
            WorkKind::Refactor => Self::Refactor,
            WorkKind::Research => Self::Research,
            WorkKind::Review => Self::Review,
            WorkKind::Docs => Self::Docs,
        }
    }
}

/// A core value this version of the bridge has no Swift case for yet (the
/// core's enums are `non_exhaustive`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0} isn't known to the Swift bridge yet")]
pub struct UnknownToBridge(pub String);

/// Fails for a kind this bridge doesn't know yet, which Swift then shows
/// as no kind.
impl TryFrom<core::WorkKind> for WorkKind {
    type Error = UnknownToBridge;

    fn try_from(kind: core::WorkKind) -> Result<Self, Self::Error> {
        match kind {
            core::WorkKind::Feat => Ok(Self::Feat),
            core::WorkKind::Fix => Ok(Self::Fix),
            core::WorkKind::Refactor => Ok(Self::Refactor),
            core::WorkKind::Research => Ok(Self::Research),
            core::WorkKind::Review => Ok(Self::Review),
            core::WorkKind::Docs => Ok(Self::Docs),
            other => Err(UnknownToBridge(other.to_string())),
        }
    }
}

impl From<PermissionMode> for core::PermissionMode {
    fn from(mode: PermissionMode) -> Self {
        match mode {
            PermissionMode::Manual => Self::Manual,
            PermissionMode::AcceptEdits => Self::AcceptEdits,
            PermissionMode::Plan => Self::Plan,
            PermissionMode::Auto => Self::Auto,
            PermissionMode::Bypass => Self::Bypass,
        }
    }
}

/// Fails for a mode this bridge doesn't know yet.
impl TryFrom<core::PermissionMode> for PermissionMode {
    type Error = UnknownToBridge;

    fn try_from(mode: core::PermissionMode) -> Result<Self, Self::Error> {
        match mode {
            core::PermissionMode::Manual => Ok(Self::Manual),
            core::PermissionMode::AcceptEdits => Ok(Self::AcceptEdits),
            core::PermissionMode::Plan => Ok(Self::Plan),
            core::PermissionMode::Auto => Ok(Self::Auto),
            core::PermissionMode::Bypass => Ok(Self::Bypass),
            other => Err(UnknownToBridge(other.to_string())),
        }
    }
}
