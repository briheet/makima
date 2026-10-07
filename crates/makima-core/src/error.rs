use crate::{AgentId, PresetId, WorkspaceId};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum RegistryError {
    #[error("name must not be empty")]
    EmptyName,
    #[error("workspace {0} is already open")]
    WorkspaceExists(WorkspaceId),
    #[error("workspace {0} not found")]
    WorkspaceNotFound(WorkspaceId),
    #[error("agent {0} not found")]
    AgentNotFound(AgentId),
    #[error("agent {0} has already finished")]
    AgentFinished(AgentId),
    #[error("no agent preset named {:?}", .0.as_str())]
    UnknownPreset(PresetId),
}
