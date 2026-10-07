use crate::{Agent, AgentId, PermissionMode, RegistryError, WorkspaceId};

/// A non-empty, trimmed workspace name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WorkspaceName(String);

impl TryFrom<String> for WorkspaceName {
    type Error = RegistryError;

    fn try_from(name: String) -> Result<Self, Self::Error> {
        Self::new(&name)
    }
}

impl From<WorkspaceName> for String {
    fn from(name: WorkspaceName) -> Self {
        name.0
    }
}

impl WorkspaceName {
    /// Validates and trims `name`.
    ///
    /// # Errors
    /// [`RegistryError::EmptyName`] if `name` is blank.
    pub fn new(name: &str) -> Result<Self, RegistryError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(RegistryError::EmptyName);
        }
        Ok(Self(trimmed.to_owned()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A piece of work and the agents organised around it.
///
/// The first agent is the **lead**, the one you talk to by default; the rest
/// are its **children**. Closing the lead promotes the next agent.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Workspace {
    id: WorkspaceId,
    name: WorkspaceName,
    /// Where the agents work.
    #[serde(default)]
    folder: Option<String>,
    /// The permission mode chosen for the whole work; `None` uses the
    /// config's, per agent. New children follow it.
    #[serde(default)]
    permission: Option<PermissionMode>,
    agents: Vec<Agent>,
}

impl Workspace {
    #[must_use]
    pub fn new(name: WorkspaceName) -> Self {
        Self {
            id: WorkspaceId::new(),
            name,
            folder: None,
            permission: None,
            agents: Vec::new(),
        }
    }

    /// The same workspace with a known id (reopening saved work).
    #[must_use]
    pub fn with_id(mut self, id: WorkspaceId) -> Self {
        self.id = id;
        self
    }

    #[must_use]
    pub fn with_folder(mut self, folder: Option<String>) -> Self {
        self.folder = folder;
        self
    }

    #[must_use]
    pub fn folder(&self) -> Option<&str> {
        self.folder.as_deref()
    }

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
    pub fn id(&self) -> WorkspaceId {
        self.id
    }

    #[must_use]
    pub fn name(&self) -> &WorkspaceName {
        &self.name
    }

    #[must_use]
    pub fn agents(&self) -> &[Agent] {
        &self.agents
    }

    /// The agent leading this work, if there are any agents.
    #[must_use]
    pub fn lead(&self) -> Option<&Agent> {
        self.agents.first()
    }

    /// Every agent except the lead, in the order they joined.
    #[must_use]
    pub fn children(&self) -> &[Agent] {
        self.agents.get(1..).unwrap_or_default()
    }

    pub(crate) fn push_agent(&mut self, agent: Agent) {
        self.agents.push(agent);
    }

    pub(crate) fn agent_mut(&mut self, id: AgentId) -> Option<&mut Agent> {
        self.agents.iter_mut().find(|a| a.id() == id)
    }

    pub(crate) fn remove_agent(&mut self, id: AgentId) -> Option<Agent> {
        let index = self.agents.iter().position(|a| a.id() == id)?;
        Some(self.agents.remove(index))
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use super::*;
    use crate::{AgentRole, AgentStatus};

    #[test]
    fn the_first_agent_leads_and_closing_it_promotes_the_next() {
        let mut workspace = Workspace::new(WorkspaceName::new("Auth").unwrap());
        assert!(workspace.lead().is_none() && workspace.children().is_empty());

        let lead = Agent::new("claude", AgentRole::Implementer, None);
        let child = Agent::new("claude 2", AgentRole::Implementer, None);
        let (lead_id, child_id) = (lead.id(), child.id());
        workspace.push_agent(lead);
        workspace.push_agent(child);
        assert_eq!(workspace.lead().map(Agent::id), Some(lead_id));
        assert_eq!(workspace.children().len(), 1);

        workspace.remove_agent(lead_id);
        assert_eq!(workspace.lead().map(Agent::id), Some(child_id));
        assert!(workspace.children().is_empty());
    }

    #[test]
    fn workspaces_round_trip_through_toml() {
        let mut workspace = Workspace::new(WorkspaceName::new("Auth").unwrap());
        let mut agent = Agent::new("claude", AgentRole::Implementer, Some("claude".into()));
        agent.set_status(AgentStatus::Failed {
            reason: "exited with code 1".into(),
        });
        workspace.push_agent(agent);

        let text = toml::to_string(&workspace).unwrap();
        assert_eq!(toml::from_str::<Workspace>(&text).unwrap(), workspace);
        // Validation runs on the way in too: a blank name is rejected.
        let blank = text.replace("name = \"Auth\"", "name = \"  \"");
        assert!(toml::from_str::<Workspace>(&blank).is_err());
    }
}
