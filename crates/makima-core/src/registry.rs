use std::collections::HashMap;

use crate::{
    Agent, AgentId, AgentPreset, AgentRole, AgentStatus, Launch, PresetId, RegistryError,
    Workspace, WorkspaceId, WorkspaceName, builtin_presets,
};

/// Every workspace and agent known to this Makima instance.
#[derive(Debug)]
pub struct Registry {
    workspaces: Vec<Workspace>,
    agent_index: HashMap<AgentId, WorkspaceId>,
    presets: Vec<AgentPreset>,
}

impl Default for Registry {
    fn default() -> Self {
        Self::with_presets(builtin_presets())
    }
}

impl Registry {
    /// A registry offering the built-in presets.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry offering `presets`, e.g. from [`crate::config::Config::presets`].
    #[must_use]
    pub fn with_presets(presets: Vec<AgentPreset>) -> Self {
        Self {
            workspaces: Vec::new(),
            agent_index: HashMap::new(),
            presets,
        }
    }

    /// Presets agents can be started from, in display order.
    #[must_use]
    pub fn presets(&self) -> &[AgentPreset] {
        &self.presets
    }

    /// The preset with `id`, if offered.
    #[must_use]
    pub fn preset(&self, id: &PresetId) -> Option<&AgentPreset> {
        self.presets.iter().find(|p| p.id() == id)
    }

    /// Workspaces in creation order.
    #[must_use]
    pub fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    #[must_use]
    pub fn workspace(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id() == id)
    }

    /// # Errors
    /// [`RegistryError::EmptyName`] if `name` is blank.
    pub fn create_workspace(&mut self, name: &str) -> Result<&Workspace, RegistryError> {
        self.insert_workspace(Workspace::new(WorkspaceName::new(name)?))
    }

    /// Adds a workspace built elsewhere (e.g. with a folder, or reopened
    /// with its saved id), with any agents it already has.
    ///
    /// # Errors
    /// [`RegistryError::WorkspaceExists`] if one with its id is open.
    pub fn insert_workspace(&mut self, workspace: Workspace) -> Result<&Workspace, RegistryError> {
        if self.workspace(workspace.id()).is_some() {
            return Err(RegistryError::WorkspaceExists(workspace.id()));
        }
        for agent in workspace.agents() {
            self.agent_index.insert(agent.id(), workspace.id());
        }
        self.workspaces.push(workspace);
        Ok(self
            .workspaces
            .last()
            .unwrap_or_else(|| unreachable!("just pushed")))
    }

    /// Closes a workspace and forgets its agents.
    ///
    /// # Errors
    /// [`RegistryError::WorkspaceNotFound`].
    pub fn remove_workspace(&mut self, id: WorkspaceId) -> Result<Workspace, RegistryError> {
        let index = self
            .workspaces
            .iter()
            .position(|w| w.id() == id)
            .ok_or(RegistryError::WorkspaceNotFound(id))?;
        let workspace = self.workspaces.remove(index);
        for agent in workspace.agents() {
            self.agent_index.remove(&agent.id());
        }
        Ok(workspace)
    }

    /// Adds an agent to a workspace. `command` is `None` for the default shell.
    ///
    /// # Errors
    /// [`RegistryError::EmptyName`] or [`RegistryError::WorkspaceNotFound`].
    pub fn add_agent(
        &mut self,
        workspace: WorkspaceId,
        name: &str,
        role: AgentRole,
        command: Option<String>,
    ) -> Result<&Agent, RegistryError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(RegistryError::EmptyName);
        }
        let ws = self.workspace_mut(workspace)?;
        let agent = Agent::new(name, role, command);
        let id = agent.id();
        ws.push_agent(agent);
        self.agent_index.insert(id, workspace);
        self.agent(id).ok_or(RegistryError::AgentNotFound(id))
    }

    /// Adds an agent from a built-in preset, named after it: "claude",
    /// "claude 2", … unique within the workspace.
    ///
    /// # Errors
    /// [`RegistryError::UnknownPreset`] or [`RegistryError::WorkspaceNotFound`].
    pub fn add_agent_from_preset(
        &mut self,
        workspace: WorkspaceId,
        preset_id: &PresetId,
    ) -> Result<&Agent, RegistryError> {
        let preset = self
            .preset(preset_id)
            .cloned()
            .ok_or_else(|| RegistryError::UnknownPreset(preset_id.clone()))?;
        let name = self.unique_agent_name(workspace, preset.base_name())?;
        self.add_agent(
            workspace,
            &name,
            preset.role().clone(),
            preset.command().map(str::to_owned),
        )
    }

    /// Starts an agent from a preset with a role and launch options (pinned
    /// session id, opening prompt). Named after the preset like
    /// [`Self::add_agent_from_preset`].
    ///
    /// # Errors
    /// [`RegistryError::UnknownPreset`] or [`RegistryError::WorkspaceNotFound`].
    pub fn start_agent(
        &mut self,
        workspace: WorkspaceId,
        preset_id: &PresetId,
        role: Option<AgentRole>,
        launch: &Launch,
    ) -> Result<&Agent, RegistryError> {
        let preset = self
            .preset(preset_id)
            .cloned()
            .ok_or_else(|| RegistryError::UnknownPreset(preset_id.clone()))?;
        let adapter = preset.adapter();
        let name = self.unique_agent_name(workspace, preset.base_name())?;
        let session = launch
            .session_id
            .clone()
            .filter(|_| adapter.pins_sessions());
        let agent = Agent::new(
            name,
            role.unwrap_or_else(|| preset.role().clone()),
            preset.command_line(launch),
        )
        .with_origin(Some(preset.id().clone()), session)
        .with_permission(launch.permission.filter(|_| adapter.supports_permissions()));
        let id = agent.id();
        self.workspace_mut(workspace)?.push_agent(agent);
        self.agent_index.insert(id, workspace);
        self.agent(id).ok_or(RegistryError::AgentNotFound(id))
    }

    /// The workspace an agent belongs to.
    #[must_use]
    pub fn workspace_of(&self, agent: AgentId) -> Option<WorkspaceId> {
        self.agent_index.get(&agent).copied()
    }

    #[must_use]
    pub fn agent(&self, id: AgentId) -> Option<&Agent> {
        let workspace = self.agent_index.get(&id)?;
        self.workspace(*workspace)?
            .agents()
            .iter()
            .find(|a| a.id() == id)
    }

    /// Moves an agent to `status`. Finished agents can't change status.
    ///
    /// # Errors
    /// [`RegistryError::AgentNotFound`] or [`RegistryError::AgentFinished`].
    pub fn set_agent_status(
        &mut self,
        id: AgentId,
        status: AgentStatus,
    ) -> Result<&Agent, RegistryError> {
        let agent = self.agent_mut(id)?;
        if agent.status().is_terminal() {
            return Err(RegistryError::AgentFinished(id));
        }
        agent.set_status(status);
        Ok(agent)
    }

    /// Records that an agent's process exited; see [`AgentStatus::from_exit`].
    ///
    /// # Errors
    /// [`RegistryError::AgentNotFound`] or [`RegistryError::AgentFinished`].
    pub fn record_exit(&mut self, id: AgentId, code: Option<i32>) -> Result<&Agent, RegistryError> {
        self.set_agent_status(id, AgentStatus::from_exit(code))
    }

    /// # Errors
    /// [`RegistryError::AgentNotFound`].
    pub fn remove_agent(&mut self, id: AgentId) -> Result<Agent, RegistryError> {
        let workspace = self
            .agent_index
            .remove(&id)
            .ok_or(RegistryError::AgentNotFound(id))?;
        self.workspace_mut(workspace)?
            .remove_agent(id)
            .ok_or(RegistryError::AgentNotFound(id))
    }

    fn agent_mut(&mut self, id: AgentId) -> Result<&mut Agent, RegistryError> {
        let workspace = *self
            .agent_index
            .get(&id)
            .ok_or(RegistryError::AgentNotFound(id))?;
        self.workspace_mut(workspace)?
            .agent_mut(id)
            .ok_or(RegistryError::AgentNotFound(id))
    }

    /// `base`, or `base N` with the smallest N ≥ 2 not taken in `workspace`.
    fn unique_agent_name(
        &self,
        workspace: WorkspaceId,
        base: &str,
    ) -> Result<String, RegistryError> {
        let agents = self
            .workspace(workspace)
            .ok_or(RegistryError::WorkspaceNotFound(workspace))?
            .agents();
        let taken = |name: &str| agents.iter().any(|a| a.name() == name);
        if !taken(base) {
            return Ok(base.to_owned());
        }
        // With n agents, one of `base 2` ..= `base n+1` is always free.
        let name = (2..=agents.len() + 1)
            .map(|n| format!("{base} {n}"))
            .find(|name| !taken(name))
            .unwrap_or_else(|| unreachable!("pigeonhole: n agents can't take n names plus base"));
        Ok(name)
    }

    fn workspace_mut(&mut self, id: WorkspaceId) -> Result<&mut Workspace, RegistryError> {
        self.workspaces
            .iter_mut()
            .find(|w| w.id() == id)
            .ok_or(RegistryError::WorkspaceNotFound(id))
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use super::*;

    fn registry_with_workspace() -> (Registry, WorkspaceId) {
        let mut registry = Registry::new();
        let id = registry.create_workspace("  Auth refactor ").unwrap().id();
        (registry, id)
    }

    #[test]
    fn workspace_names_are_trimmed_and_non_empty() {
        let (registry, id) = registry_with_workspace();
        assert_eq!(
            registry.workspace(id).unwrap().name().as_str(),
            "Auth refactor"
        );
        assert_eq!(
            Registry::new().create_workspace("   ").unwrap_err(),
            RegistryError::EmptyName
        );
    }

    #[test]
    fn a_workspace_is_inserted_once_with_its_agents() {
        let mut registry = Registry::new();
        let mut workspace = Workspace::new(WorkspaceName::new("Auth").unwrap());
        let agent = Agent::new("claude", AgentRole::Implementer, None);
        let agent_id = agent.id();
        workspace.push_agent(agent);
        let id = workspace.id();
        registry.insert_workspace(workspace.clone()).unwrap();
        assert_eq!(registry.workspace_of(agent_id), Some(id));
        assert_eq!(
            registry.insert_workspace(workspace).unwrap_err(),
            RegistryError::WorkspaceExists(id)
        );
        assert_eq!(registry.workspaces().len(), 1);

        registry.remove_workspace(id).unwrap();
        assert_eq!(registry.workspace_of(agent_id), None);
        assert_eq!(
            registry.remove_workspace(id).unwrap_err(),
            RegistryError::WorkspaceNotFound(id)
        );
    }

    #[test]
    fn added_agents_start_in_starting() {
        let (mut registry, ws) = registry_with_workspace();
        let agent = registry
            .add_agent(ws, "shell", AgentRole::Shell, None)
            .unwrap()
            .clone();
        assert_eq!(agent.status(), &AgentStatus::Starting);
        assert_eq!(registry.workspace(ws).unwrap().agents(), &[agent]);
    }

    #[test]
    fn adding_to_a_missing_workspace_fails() {
        let mut registry = Registry::new();
        let missing = WorkspaceId::new();
        assert_eq!(
            registry
                .add_agent(missing, "shell", AgentRole::Shell, None)
                .unwrap_err(),
            RegistryError::WorkspaceNotFound(missing)
        );
    }

    #[test]
    fn finished_agents_keep_their_status() {
        let (mut registry, ws) = registry_with_workspace();
        let id = registry
            .add_agent(ws, "tests", AgentRole::Tester, Some("cargo test".into()))
            .unwrap()
            .id();
        registry
            .set_agent_status(id, AgentStatus::Completed)
            .unwrap();
        assert_eq!(
            registry
                .set_agent_status(id, AgentStatus::Working)
                .unwrap_err(),
            RegistryError::AgentFinished(id)
        );
    }

    #[test]
    fn preset_agents_get_unique_names() {
        let (mut registry, ws) = registry_with_workspace();
        let names: Vec<String> = (0..3)
            .map(|_| {
                registry
                    .add_agent_from_preset(ws, &"claude".into())
                    .unwrap()
                    .name()
                    .to_owned()
            })
            .collect();
        assert_eq!(names, ["claude", "claude 2", "claude 3"]);
        let agent = registry
            .agent(registry.workspace(ws).unwrap().agents()[0].id())
            .unwrap();
        assert_eq!(agent.command(), Some("claude"));
        assert_eq!(
            registry
                .add_agent_from_preset(ws, &"nope".into())
                .unwrap_err(),
            RegistryError::UnknownPreset("nope".into())
        );
    }

    #[test]
    fn exits_map_to_completed_or_failed() {
        let (mut registry, ws) = registry_with_workspace();
        let ok = registry
            .add_agent_from_preset(ws, &"shell".into())
            .unwrap()
            .id();
        let bad = registry
            .add_agent_from_preset(ws, &"claude".into())
            .unwrap()
            .id();
        assert_eq!(
            registry.record_exit(ok, None).unwrap().status(),
            &AgentStatus::Completed
        );
        assert_eq!(
            registry.record_exit(bad, Some(127)).unwrap().status(),
            &AgentStatus::Failed {
                reason: "exited with code 127".into()
            }
        );
    }

    #[test]
    fn removed_agents_are_gone() {
        let (mut registry, ws) = registry_with_workspace();
        let id = registry
            .add_agent(ws, "shell", AgentRole::Shell, None)
            .unwrap()
            .id();
        registry.remove_agent(id).unwrap();
        assert!(registry.agent(id).is_none());
        assert!(registry.workspace(ws).unwrap().agents().is_empty());
    }
}
