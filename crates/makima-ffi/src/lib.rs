//! The Swift-facing API of the Makima core.
//!
//! Everything here is a thin translation layer: plain data crosses as
//! records, the core lives behind one [`Core`] object, and changes are
//! pushed to Swift through [`CoreEventSink`]. Calls are synchronous; see
//! `docs/rust-swift-practices.md` for why there is no exported `async fn`.
//!
//! IDs cross as typed values, not strings: [`AgentId`] and [`WorkspaceId`]
//! are custom types that Swift sees as `AgentID` and `WorkspaceID`
//! (configured in `uniffi.toml`); [`PresetId`] crosses as a string.
//!
//! Failures the API recovers from (an unreadable store, a saved work this
//! version can't read) are logged through `tracing` to the unified log,
//! subsystem `io.makima.app`, category `core`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use makima_core::{AgentId, PresetId, Registry, RegistryError, Template, WorkspaceId};
use makima_store::{Paths, Store, WorkDb};

mod config;
mod design;
mod tui;
mod types;
mod work;

pub use config::{
    AppearanceMode, AppearanceSettings, ConfigInfo, CursorShape, LeaderKey, LoadedConfig,
    NavigateKeys, TerminalSettings, ToolInfo, VoiceProviderKind, VoiceSettings,
    default_config_text, load_config,
};
pub use design::{
    AccentTokens, BorderTokens, DesignTokensInfo, InteractiveTokens, StatusTokens, SurfaceTokens,
    TextTokens, ThemePaletteInfo, derive_design_tokens,
};
pub use tui::{
    ChromeCellRect, ChromeCounts, ChromeFloat, ChromeFrame, ChromeMode, ChromeOverlay, ChromePane,
    ChromePaneRect, ChromePaneRole, ChromePaneStatus, ChromeState, ChromeTab, ChromeTabHit,
    ChromeTalkLine, FocusDirection, InputError, KeyAction, KeyBinding, KeyOutcome, KeyStyle,
    KeymapEngine, MakimaCommand, Picker, PickerEvent, PickerItemInfo, RoutedTalk, TalkEditor,
    TalkEditorEvent, TalkRecipient, TalkTarget, ToolKeyInfo, neighbor_pane, parse_makima_command,
    render_chrome, route_talk,
};
pub use types::{
    AgentInfo, AgentPresetInfo, AgentRole, AgentStatus, CoreEvent, PermissionMode, UnknownToBridge,
    WorkKind, WorkspaceInfo,
};
pub use work::{
    BriefAnswer, ChildRequestInfo, FlowEventInfo, NewWorkFlow, NewWorkRequestInfo, RecentWorkInfo,
    StorageMode, VoiceClipInfo,
};

uniffi::setup_scaffolding!();

uniffi::custom_type!(AgentId, String, {
    remote,
    lower: |id| id.to_string(),
    try_lift: |text| Ok(text.parse()?),
});

uniffi::custom_type!(WorkspaceId, String, {
    remote,
    lower: |id| id.to_string(),
    try_lift: |text| Ok(text.parse()?),
});

uniffi::custom_type!(PresetId, String, {
    remote,
    lower: |id| id.as_str().to_owned(),
    try_lift: |text| Ok(PresetId::new(text)),
});

/// Sends `tracing` events to the unified log, once per process.
fn init_logging() {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Fails only if the host already installed a subscriber; keep it.
        let _ = tracing_subscriber::registry()
            .with(tracing_oslog::OsLogger::new("io.makima.app", "core"))
            .try_init();
    });
}

/// `error` and its sources, outermost first: `database: disk I/O error`.
/// Swift gets messages, not Rust error values, so the chain is spelled out.
pub(crate) fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[non_exhaustive]
pub enum CoreError {
    #[error("name must not be empty")]
    EmptyName,
    #[error("workspace {id} not found")]
    WorkspaceNotFound { id: WorkspaceId },
    #[error("agent {id} not found")]
    AgentNotFound { id: AgentId },
    #[error("agent {id} has already finished")]
    AgentFinished { id: AgentId },
    #[error("no agent preset named {:?}", id.as_str())]
    UnknownPreset { id: PresetId },
    #[error("invalid colour {value:?}")]
    InvalidColor { value: String },
    /// No saved work has this id.
    #[error("no saved work {id}")]
    UnknownWork { id: WorkspaceId },
    /// Storage is off (`StorageMode::Off`), so there's no saved work.
    #[error("storage is off")]
    StorageOff,
    /// Saving or loading work failed; `message` spells out every cause.
    #[error("storage: {message}")]
    Storage { message: String },
    /// A core error this version of the bridge has no case for yet, with
    /// its message.
    #[error("{message}")]
    Other { message: String },
}

impl From<RegistryError> for CoreError {
    fn from(error: RegistryError) -> Self {
        match error {
            RegistryError::EmptyName => Self::EmptyName,
            RegistryError::WorkspaceNotFound(id) => Self::WorkspaceNotFound { id },
            RegistryError::AgentNotFound(id) => Self::AgentNotFound { id },
            RegistryError::AgentFinished(id) => Self::AgentFinished { id },
            RegistryError::UnknownPreset(id) => Self::UnknownPreset { id },
            // Callers that insert workspaces handle this one; reaching here
            // keeps its message.
            // `RegistryError` is non_exhaustive; keep its message rather
            // than pretending it's one of the variants above.
            other => Self::Other {
                message: other.to_string(),
            },
        }
    }
}

/// Receives every change to the core. Implemented in Swift; called on the
/// thread that made the change, after the core's lock is released.
#[uniffi::export(with_foreign)]
pub trait CoreEventSink: Send + Sync {
    fn on_event(&self, event: CoreEvent);
}

/// The root object Swift holds for the app's lifetime.
#[derive(uniffi::Object)]
pub struct Core {
    registry: Mutex<Registry>,
    sink: Arc<dyn CoreEventSink>,
    templates: Vec<Template>,
    /// `[work] lead`: what new work's lead runs unless told otherwise.
    default_lead: PresetId,
    /// Finds repos under `[work] roots` for the folder picker.
    repos: Arc<makima_store::RepoScan>,
    /// `[permissions]`: the mode for agents whose work didn't choose one.
    permissions: makima_core::config::Permissions,
    /// Helix style makes forms modal (`j`/`k`, `i`, Esc).
    modal_forms: bool,
    /// Where programs save sessions, to resume only the ones they have.
    sessions: makima_store::Sessions,
    /// `None` when storage is off.
    store: Option<Store>,
    /// Each open workspace's database.
    works: Mutex<HashMap<WorkspaceId, Arc<WorkDb>>>,
}

#[uniffi::export]
impl Core {
    /// A core configured from the config file at `config_path` (built-in
    /// defaults when `None`), keeping work according to `storage`. If the
    /// store can't be opened, work isn't saved; the app still runs.
    #[uniffi::constructor]
    #[must_use]
    pub fn new(
        sink: Arc<dyn CoreEventSink>,
        config_path: Option<String>,
        storage: StorageMode,
    ) -> Arc<Self> {
        Self::build(sink, config_path, storage, makima_store::Sessions::find())
    }

    /// Presets agents can be started from, in display order.
    #[must_use]
    pub fn presets(&self) -> Vec<AgentPresetInfo> {
        self.registry().presets().iter().map(Into::into).collect()
    }

    /// The preset new work's lead runs when none is named (`[work] lead`).
    #[must_use]
    pub fn default_lead(&self) -> PresetId {
        self.default_lead.clone()
    }

    /// Every workspace, in creation order.
    #[must_use]
    pub fn workspaces(&self) -> Vec<WorkspaceInfo> {
        self.registry()
            .workspaces()
            .iter()
            .map(Into::into)
            .collect()
    }

    /// Adds an agent to a workspace. A `None` command runs the user's shell.
    ///
    /// # Errors
    /// [`CoreError::WorkspaceNotFound`] or [`CoreError::EmptyName`].
    pub fn add_agent(
        &self,
        workspace_id: WorkspaceId,
        name: &str,
        role: AgentRole,
        command: Option<String>,
    ) -> Result<AgentInfo, CoreError> {
        let agent: AgentInfo = self
            .registry()
            .add_agent(workspace_id, name, role.into(), command)?
            .into();
        self.sink.on_event(CoreEvent::AgentAdded {
            workspace_id,
            agent: agent.clone(),
        });
        Ok(agent)
    }

    /// Adds an agent from a preset, named "claude", "claude 2", …, with a
    /// pinned session and the work's permission mode (or the config's),
    /// and saves it with the work.
    ///
    /// # Errors
    /// [`CoreError::WorkspaceNotFound`], [`CoreError::UnknownPreset`] or
    /// [`CoreError::Storage`].
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI passes custom types by value"
    )]
    pub fn add_preset_agent(
        &self,
        workspace_id: WorkspaceId,
        preset_id: PresetId,
    ) -> Result<AgentInfo, CoreError> {
        let mut registry = self.registry();
        let permission = registry
            .workspace(workspace_id)
            .and_then(makima_core::Workspace::permission)
            .or_else(|| self.permissions.mode_for(&preset_id));
        let launch = makima_core::Launch::new()
            .with_session(uuid::Uuid::new_v4().to_string())
            .with_permission(permission);
        let agent = registry
            .start_agent(workspace_id, &preset_id, None, &launch)?
            .clone();
        let position = registry
            .workspace(workspace_id)
            .map_or(0, |w| w.agents().len().saturating_sub(1));
        drop(registry);
        if let Some(work) = self.works().get(&workspace_id).cloned() {
            Self::save_agent(&work, position, &agent)?;
        }
        let agent: AgentInfo = (&agent).into();
        self.sink.on_event(CoreEvent::AgentAdded {
            workspace_id,
            agent: agent.clone(),
        });
        Ok(agent)
    }

    /// Records that an agent's process exited. `exit_code` is `None` when
    /// unknown; non-zero codes mark the agent failed.
    ///
    /// # Errors
    /// [`CoreError::AgentNotFound`] or [`CoreError::AgentFinished`].
    pub fn record_agent_exit(
        &self,
        agent_id: AgentId,
        exit_code: Option<i32>,
    ) -> Result<AgentInfo, CoreError> {
        let agent: AgentInfo = self.registry().record_exit(agent_id, exit_code)?.into();
        if let Some(work) = self.work_of(agent_id) {
            work.end_agent(agent_id, exit_code)?;
        }
        self.sink.on_event(CoreEvent::AgentUpdated {
            agent: agent.clone(),
        });
        Ok(agent)
    }

    /// # Errors
    /// [`CoreError::AgentNotFound`] or [`CoreError::AgentFinished`].
    pub fn set_agent_status(
        &self,
        agent_id: AgentId,
        status: AgentStatus,
    ) -> Result<AgentInfo, CoreError> {
        let status = makima_core::AgentStatus::from(status);
        let detail = status.to_string();
        let agent: AgentInfo = self.registry().set_agent_status(agent_id, status)?.into();
        if let Some(work) = self.work_of(agent_id) {
            work.event(Some(agent_id), "agent.status", Some(&detail))?;
        }
        self.sink.on_event(CoreEvent::AgentUpdated {
            agent: agent.clone(),
        });
        Ok(agent)
    }

    /// # Errors
    /// [`CoreError::AgentNotFound`].
    pub fn remove_agent(&self, agent_id: AgentId) -> Result<(), CoreError> {
        self.registry().remove_agent(agent_id)?;
        self.sink.on_event(CoreEvent::AgentRemoved { agent_id });
        Ok(())
    }
}

impl Drop for Core {
    /// Ends the repo scan rather than leaving it walking the disk.
    fn drop(&mut self) {
        self.repos.stop();
    }
}

impl std::fmt::Debug for Core {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Core")
            .field("registry", &self.registry)
            .field("templates", &self.templates.len())
            .field("default_lead", &self.default_lead)
            .field("permissions", &self.permissions)
            .field("modal_forms", &self.modal_forms)
            .field("sessions", &self.sessions)
            .field("store", &self.store)
            .finish_non_exhaustive()
    }
}

impl Core {
    /// Locks the registry. A panic while holding the lock leaves the
    /// registry consistent (every mutation is a single step), so poisoning
    /// is ignored rather than wedging the app.
    fn registry(&self) -> MutexGuard<'_, Registry> {
        self.registry.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Open workspaces' databases. Inserts are single steps, so poisoning
    /// is ignored.
    fn works(&self) -> MutexGuard<'_, HashMap<WorkspaceId, Arc<WorkDb>>> {
        self.works.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The database of the workspace `agent` belongs to, if it's saved.
    fn work_of(&self, agent: AgentId) -> Option<Arc<WorkDb>> {
        let workspace = self.registry().workspace_of(agent)?;
        self.works().get(&workspace).cloned()
    }
}

impl Core {
    /// [`Core::new`] with where programs save sessions given (tests use a
    /// temporary folder).
    pub(crate) fn build(
        sink: Arc<dyn CoreEventSink>,
        config_path: Option<String>,
        storage: StorageMode,
        sessions: makima_store::Sessions,
    ) -> Arc<Self> {
        init_logging();
        let config = match config_path {
            Some(path) => config::read_config(Some(path.into())).0,
            None => makima_core::config::Config::default(),
        };
        let paths = match storage {
            StorageMode::Off => None,
            StorageMode::Default => Paths::resolve(config.storage.root.as_deref())
                .inspect_err(|e| {
                    tracing::error!(error = error_chain(e), "no data folder; work isn't saved");
                })
                .ok(),
            StorageMode::At { root } => Some(Paths::under(std::path::Path::new(&root))),
        };
        let store = paths.and_then(|p| {
            Store::open(p)
                .inspect_err(|e| {
                    tracing::error!(
                        error = error_chain(e),
                        "can't open the store; work isn't saved"
                    );
                })
                .ok()
        });
        Arc::new(Self {
            registry: Mutex::new(Registry::with_presets(config.presets())),
            sink,
            templates: config.templates(),
            default_lead: config.work.lead.clone(),
            repos: makima_store::RepoScan::new(
                config
                    .work
                    .roots
                    .iter()
                    .filter_map(|r| makima_store::expand_tilde(r).ok())
                    .collect(),
                config.work.depth,
                2_000,
            ),
            permissions: config.permissions.clone(),
            modal_forms: config.keys.style != makima_core::config::KeyStyle::Tmux,
            sessions,
            store,
            works: Mutex::new(HashMap::new()),
        })
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use super::*;

    /// An empty workspace, straight in the registry.
    fn workspace(core: &Core, name: &str) -> WorkspaceInfo {
        core.registry().create_workspace(name).unwrap().into()
    }

    #[derive(Default)]
    struct RecordingSink(Mutex<Vec<CoreEvent>>);

    impl CoreEventSink for RecordingSink {
        fn on_event(&self, event: CoreEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    #[test]
    fn changes_are_reported_to_the_sink() {
        let sink = Arc::new(RecordingSink::default());
        let core = Core::new(sink.clone(), None, StorageMode::Off);

        let ws = workspace(&core, "Scratch");
        let agent = core
            .add_agent(ws.id, "shell", AgentRole::Shell, None)
            .unwrap();
        core.set_agent_status(agent.id, AgentStatus::Working)
            .unwrap();
        core.remove_agent(agent.id).unwrap();

        // Added, updated, removed (the helper adds the workspace quietly).
        let events = sink.0.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(
            matches!(&events[1], CoreEvent::AgentUpdated { agent } if agent.status == AgentStatus::Working)
        );
        assert!(core.workspaces()[0].agents.is_empty());
    }

    #[test]
    fn preset_agents_exit() {
        let core = Core::new(Arc::new(RecordingSink::default()), None, StorageMode::Off);
        let ws = workspace(&core, "Agents");
        let claude = core.add_preset_agent(ws.id, "claude".into()).unwrap();
        let session = claude.session_id.clone().unwrap();
        assert_eq!(
            claude.command.as_deref(),
            Some(format!("claude --session-id {session}").as_str()),
            "children are resumable too"
        );

        let failed = core.record_agent_exit(claude.id, Some(2)).unwrap();
        assert!(matches!(failed.status, AgentStatus::Failed { .. }));
        assert!(core.presets().iter().any(|p| p.id == "codex"));
    }

    #[test]
    fn unknown_ids_are_not_found() {
        let core = Core::new(Arc::new(RecordingSink::default()), None, StorageMode::Off);
        let missing = AgentId::new();
        assert!(matches!(
            core.remove_agent(missing),
            Err(CoreError::AgentNotFound { id }) if id == missing
        ));
    }

    #[test]
    fn every_registry_error_keeps_its_meaning() {
        let id = AgentId::new();
        assert!(matches!(
            CoreError::from(RegistryError::EmptyName),
            CoreError::EmptyName
        ));
        assert!(matches!(
            CoreError::from(RegistryError::AgentFinished(id)),
            CoreError::AgentFinished { .. }
        ));
    }
}
