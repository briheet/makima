//! Starting, reopening and recording work: the new-work flow for Swift,
//! and the `Core` methods that persist each work to its own `SQLite`
//! database (see `makima-store`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use makima_core::brief::{Brief, BriefNote};
use makima_core::prompt;
use makima_core::{
    self as core, AgentId, Launch, PresetId, Registry, RegistryError, Workspace, WorkspaceId,
    WorkspaceName,
};
use makima_store::{AgentRow, NewWork, RepoScan, StoreError, WorkDb, expand_tilde};
use makima_tui as tui;

use crate::{
    AgentRole, Core, CoreError, CoreEvent, PermissionMode, WorkKind, WorkspaceInfo, error_chain,
};

/// Where a [`Core`] keeps work.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum StorageMode {
    /// Nothing is saved (tests, previews).
    Off,
    /// `~/.local/share/makima`, or the config's `[storage] root`.
    Default,
    /// Everything under `root` (portable setups, tests).
    At { root: String },
}

/// A transcribed voice recording.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VoiceClipInfo {
    /// The recording file; moved into the work when saved.
    pub recording: String,
    pub duration_ms: Option<u64>,
    pub transcript: String,
    /// The brief question it answers; `None` for a free note.
    pub question: Option<String>,
    /// e.g. `apple`.
    pub provider: String,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChildRequestInfo {
    pub preset: PresetId,
    pub role: AgentRole,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BriefAnswer {
    pub question: String,
    pub answer: String,
}

/// Everything the new-work flow collected.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NewWorkRequestInfo {
    pub name: String,
    pub folder: String,
    pub template: Option<String>,
    pub kind: Option<WorkKind>,
    pub lead: PresetId,
    pub children: Vec<ChildRequestInfo>,
    pub read_only: bool,
    /// The whole team's mode; `None` resolves per agent from the config.
    pub permission: Option<PermissionMode>,
    pub answers: Vec<BriefAnswer>,
    pub voice: Vec<VoiceClipInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum FlowEventInfo {
    Start { request: NewWorkRequestInfo },
    Reopen { work_id: WorkspaceId },
    Close,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RecentWorkInfo {
    pub id: WorkspaceId,
    pub name: String,
    pub kind: Option<WorkKind>,
    pub folder: Option<String>,
}

/// The welcome screen, template picker and new-work form, fed raw key
/// bytes from the chrome terminal. Made by [`Core::new_work_flow`].
#[derive(Debug, uniffi::Object)]
pub struct NewWorkFlow {
    flow: Mutex<tui::NewWorkFlow>,
    /// Repos for the folder picker, added once the background scan is done.
    repos: Arc<RepoScan>,
    repos_added: AtomicBool,
}

#[uniffi::export]
impl NewWorkFlow {
    /// Applies one chunk of terminal input.
    #[must_use]
    pub fn feed(&self, input: &[u8]) -> Vec<FlowEventInfo> {
        self.flow()
            .feed(input)
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Adds a transcribed voice note to the focused question, or as a note.
    pub fn add_voice(&self, clip: VoiceClipInfo) {
        self.flow().add_voice(clip.into());
    }

    /// Shows a one-line message, e.g. why starting failed.
    pub fn set_notice(&self, notice: Option<String>) {
        self.flow().set_notice(notice);
    }

    /// Shows "● recording", "transcribing …", or nothing.
    pub fn set_voice_status(&self, status: Option<String>) {
        self.flow().set_voice_status(status);
    }
}

impl NewWorkFlow {
    /// The flow's state, e.g. for rendering, with the repo scan's results
    /// once they're in. A panic mid-key leaves at worst a half-applied
    /// keystroke, so poisoning is ignored.
    pub(crate) fn flow(&self) -> MutexGuard<'_, tui::NewWorkFlow> {
        let mut flow = self.flow.lock().unwrap_or_else(PoisonError::into_inner);
        if !self.repos_added.load(Ordering::Acquire)
            && let Some(repos) = self.repos.repos()
        {
            flow.add_folders(repos.into_iter().map(|r| r.display().to_string()));
            self.repos_added.store(true, Ordering::Release);
        }
        flow
    }
}

impl From<VoiceClipInfo> for tui::VoiceClip {
    fn from(clip: VoiceClipInfo) -> Self {
        Self::new(clip.recording, clip.transcript, clip.provider)
            .with_duration_ms(clip.duration_ms)
            .with_language(clip.language)
            .for_question(clip.question)
    }
}

impl From<tui::VoiceClip> for VoiceClipInfo {
    fn from(clip: tui::VoiceClip) -> Self {
        Self {
            recording: clip.recording,
            duration_ms: clip.duration_ms,
            transcript: clip.transcript,
            question: clip.question,
            provider: clip.provider,
            language: clip.language,
        }
    }
}

impl From<tui::NewWorkRequest> for NewWorkRequestInfo {
    fn from(request: tui::NewWorkRequest) -> Self {
        Self {
            name: request.name,
            folder: request.folder,
            template: request.template,
            kind: request.kind.and_then(|k| WorkKind::try_from(k).ok()),
            lead: request.lead,
            children: request
                .children
                .into_iter()
                .map(|c| ChildRequestInfo {
                    preset: c.preset,
                    role: (&c.role).into(),
                })
                .collect(),
            read_only: request.read_only,
            permission: request
                .permission
                .and_then(|m| PermissionMode::try_from(m).ok()),
            answers: request
                .answers
                .into_iter()
                .map(|(question, answer)| BriefAnswer { question, answer })
                .collect(),
            voice: request.voice.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<tui::FlowEvent> for FlowEventInfo {
    fn from(event: tui::FlowEvent) -> Self {
        match event {
            tui::FlowEvent::Start(request) => Self::Start {
                request: request.into(),
            },
            tui::FlowEvent::Reopen(work_id) => Self::Reopen { work_id },
            // Close, and any event the flow (non_exhaustive) gains later.
            _ => Self::Close,
        }
    }
}

/// The team for prompts: `lead (claude) + tester (claude) + 6 ×
/// implementer (claude)`, repeats counted.
fn team_summary(request: &NewWorkRequestInfo) -> String {
    let mut parts: Vec<(String, usize)> = vec![(format!("lead ({})", request.lead), 1)];
    for child in &request.children {
        let role = core::AgentRole::from(child.role.clone());
        let part = format!("{role} ({})", child.preset);
        match parts.iter_mut().find(|(p, _)| *p == part) {
            Some((_, count)) => *count += 1,
            None => parts.push((part, 1)),
        }
    }
    parts
        .into_iter()
        .map(|(part, count)| {
            if count == 1 {
                part
            } else {
                format!("{count} × {part}")
            }
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

impl From<StoreError> for CoreError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::UnknownWork(id) => Self::UnknownWork { id },
            other => Self::Storage {
                message: error_chain(&other),
            },
        }
    }
}

#[uniffi::export]
impl Core {
    /// A new-work flow offering this core's presets, templates, recent work
    /// and folders. `can_close` lets Esc leave it (there's running work).
    #[must_use]
    pub fn new_work_flow(&self, can_close: bool) -> Arc<NewWorkFlow> {
        let home = expand_tilde("~")
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let recents = self
            .saved_works(9)
            .into_iter()
            .map(|w| tui::RecentWork::new(w.id, w.name, w.kind.map(Into::into), w.folder))
            .collect();
        let presets = self.registry().presets().to_vec();
        let options = tui::FlowOptions::new(presets, self.templates.clone())
            .with_default_preset(self.default_lead.clone())
            .with_home(home)
            .with_folders(self.frecent_folders())
            .with_recents(recents)
            .with_config_permission(self.permissions.default)
            .modal(self.modal_forms)
            .closable(can_close);
        // A fresh scan each time, so new clones show up; the last one is
        // used until it finishes.
        if let Err(error) = self.repos.refresh() {
            tracing::warn!(%error, "can't start the repo scan; offering recent folders only");
        }
        Arc::new(NewWorkFlow {
            flow: Mutex::new(tui::NewWorkFlow::new(options)),
            repos: Arc::clone(&self.repos),
            repos_added: AtomicBool::new(false),
        })
    }

    /// Starts work: saves it (its own folder and database, the brief, voice
    /// notes), then starts the lead and children in its folder. If any of
    /// that fails, nothing is left behind: no open workspace, no saved work.
    ///
    /// # Errors
    /// [`CoreError::EmptyName`], [`CoreError::UnknownPreset`] or
    /// [`CoreError::Storage`].
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI passes records by value"
    )]
    pub fn start_work(&self, request: NewWorkRequestInfo) -> Result<WorkspaceInfo, CoreError> {
        let name = WorkspaceName::new(&request.name)?;
        let folder = expand_tilde(&request.folder)?.display().to_string();
        let permission: Option<core::PermissionMode> = request.permission.map(Into::into);
        let workspace = Workspace::new(name)
            .with_folder(Some(folder.clone()))
            .with_permission(permission);
        let workspace_id = workspace.id();
        let kind: Option<core::WorkKind> = request.kind.map(Into::into);

        let work = match &self.store {
            Some(store) => Some(
                store.create_work(
                    &NewWork::new(workspace_id, request.name.clone())
                        .with_kind(kind)
                        .with_template(request.template.clone())
                        .with_folder(Some(folder.clone()))
                        .read_only(request.read_only)
                        .with_permission(permission),
                )?,
            ),
            None => None,
        };
        let started = self.start_team(&request, workspace, kind, &folder, work.as_ref());
        let info = match started {
            Ok(info) => info,
            Err(error) => {
                self.forget_work(workspace_id, work);
                return Err(error);
            }
        };
        if let Some(work) = work {
            self.works().insert(workspace_id, Arc::new(work));
        }
        self.sink.on_event(CoreEvent::WorkspaceCreated {
            workspace: info.clone(),
        });
        Ok(info)
    }

    /// Reopens saved work: the same folder and team, resuming each agent's
    /// session where its program supports it (Claude Code's `--resume`).
    /// Returns the open workspace if it's already open.
    ///
    /// # Errors
    /// [`CoreError::UnknownWork`], [`CoreError::StorageOff`],
    /// [`CoreError::Storage`] if it can't be read, or
    /// [`CoreError::UnknownPreset`] for a preset no longer configured.
    pub fn reopen_work(&self, work_id: WorkspaceId) -> Result<WorkspaceInfo, CoreError> {
        if let Some(open) = self.registry().workspace(work_id) {
            return Ok(open.into());
        }
        let store = self.store.as_ref().ok_or(CoreError::StorageOff)?;
        let work = Arc::new(store.open_work(work_id)?);
        let row = work.work()?;
        let saved_agents = work.agents()?;

        let mut registry = self.registry();
        let workspace = Workspace::new(WorkspaceName::new(&row.name)?)
            .with_id(work_id)
            .with_folder(row.folder.clone())
            .with_permission(row.permission);
        match registry.insert_workspace(workspace) {
            Ok(_) => {}
            // Another call reopened it while this one read the store.
            Err(RegistryError::WorkspaceExists(_)) => {
                return registry
                    .workspace(work_id)
                    .map(Into::into)
                    .ok_or(CoreError::WorkspaceNotFound { id: work_id });
            }
            Err(error) => return Err(error.into()),
        }
        let started = self.restart_team(&mut registry, work_id, &saved_agents);
        let agents = match started {
            Ok(agents) => agents,
            Err(error) => {
                // Close it again; the saved work stays as it was.
                let _ = registry.remove_workspace(work_id);
                return Err(error);
            }
        };
        let info: WorkspaceInfo = registry
            .workspace(work_id)
            .ok_or(CoreError::WorkspaceNotFound { id: work_id })?
            .into();
        drop(registry);

        work.event(None, "work.reopened", None)?;
        Self::save_agents(&work, &agents)?;
        self.works().insert(work_id, work);
        self.sink.on_event(CoreEvent::WorkspaceCreated {
            workspace: info.clone(),
        });
        Ok(info)
    }

    /// Records a message the user sent to agents, with its voice recording
    /// if it was spoken.
    ///
    /// # Errors
    /// [`CoreError::Storage`].
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI passes records by value"
    )]
    pub fn record_message(
        &self,
        workspace_id: WorkspaceId,
        text: &str,
        recipients: Vec<AgentId>,
        voice: Option<VoiceClipInfo>,
    ) -> Result<(), CoreError> {
        let Some(work) = self.works().get(&workspace_id).cloned() else {
            return Ok(());
        };
        let audio = match &voice {
            Some(clip) => Some(Self::save_clip(&work, clip)?),
            None => None,
        };
        work.add_message(text, &recipients, audio.as_deref())?;
        Ok(())
    }

    /// Where to record voice before it belongs to a work.
    #[must_use]
    pub fn recordings_dir(&self) -> Option<String> {
        self.store
            .as_ref()
            .map(|s| s.paths().recordings_dir.display().to_string())
    }
}

impl Core {
    /// Saves the brief, opens the workspace and starts its team, telling
    /// each agent its job for template work. Returns the open workspace.
    fn start_team(
        &self,
        request: &NewWorkRequestInfo,
        workspace: Workspace,
        kind: Option<core::WorkKind>,
        folder: &str,
        work: Option<&WorkDb>,
    ) -> Result<WorkspaceInfo, CoreError> {
        let workspace_id = workspace.id();
        let permission = workspace.permission();
        let mode_for = |preset: &PresetId| permission.or_else(|| self.permissions.mode_for(preset));
        let brief_path = match work {
            Some(work) => Self::save_brief(work, request, kind, folder)?,
            None => None,
        };
        let brief_path = brief_path.map(|p| p.display().to_string());

        // Template work tells every agent its job; quick starts say nothing.
        let template = request
            .template
            .as_ref()
            .and_then(|id| self.templates.iter().find(|t| &t.id == id));
        let team = team_summary(request);
        let plan = brief_path.as_deref().map_or_else(
            || "(no plan file)".to_owned(),
            |brief| {
                std::path::Path::new(brief)
                    .with_file_name("plan.md")
                    .display()
                    .to_string()
            },
        );
        let vars = prompt::PromptVars {
            name: &request.name,
            kind: kind.map_or("work", core::WorkKind::id),
            folder,
            brief: brief_path.as_deref().unwrap_or("(no brief)"),
            team: &team,
            plan: &plan,
            ..prompt::PromptVars::default()
        };
        let caution = if request.read_only {
            " Do not modify files."
        } else {
            ""
        };
        let opening = |text: &str, vars: &prompt::PromptVars<'_>| {
            format!("{}{caution}", prompt::render(text, vars))
        };

        let mut registry = self.registry();
        registry.insert_workspace(workspace)?;
        let session = || uuid::Uuid::new_v4().to_string();
        let lead_launch = Launch::new()
            .with_session(session())
            .with_prompt(template.map(|t| opening(&t.lead_prompt, &vars)))
            .with_permission(mode_for(&request.lead));
        let lead_role = request.template.as_ref().map(|_| core::AgentRole::Planner);
        registry.start_agent(workspace_id, &request.lead, lead_role, &lead_launch)?;
        for (index, child) in request.children.iter().enumerate() {
            let role = core::AgentRole::from(child.role.clone());
            let role_word = role.to_string();
            // Numbered as the panes show them: 1 is the first child.
            let number = (index + 1).to_string();
            let child_vars = prompt::PromptVars {
                number: &number,
                role: &role_word,
                duty: prompt::duty(&role),
                ..vars
            };
            let launch = Launch::new()
                .with_session(session())
                .with_prompt(template.map(|t| opening(&t.child_prompt, &child_vars)))
                .with_permission(mode_for(&child.preset));
            registry.start_agent(workspace_id, &child.preset, Some(role), &launch)?;
        }
        let workspace = registry
            .workspace(workspace_id)
            .ok_or(CoreError::WorkspaceNotFound { id: workspace_id })?;
        let info: WorkspaceInfo = workspace.into();
        let agents = workspace.agents().to_vec();
        drop(registry);

        if let Some(work) = work {
            Self::save_agents(work, &agents)?;
        }
        Ok(info)
    }

    /// Starts a reopened work's saved team: the latest run of each
    /// position, resuming sessions its program saved.
    fn restart_team(
        &self,
        registry: &mut Registry,
        work_id: WorkspaceId,
        saved: &[AgentRow],
    ) -> Result<Vec<core::Agent>, CoreError> {
        // Reopening adds rows, it doesn't replace them.
        let mut latest: Vec<&AgentRow> = Vec::new();
        for agent in saved {
            match latest.iter_mut().find(|a| a.position == agent.position) {
                Some(slot) => *slot = agent,
                None => latest.push(agent),
            }
        }
        let shell = PresetId::new("shell");
        for agent in latest {
            let preset = agent.preset.as_ref().unwrap_or(&shell);
            let launch = Launch::new().with_permission(agent.permission);
            // Claude only saves a session once it gets a message; one that
            // never did starts fresh with the same id instead of failing.
            let launch = match &agent.session_id {
                Some(id) if self.session_saved(registry, preset, id) => {
                    launch.resuming(Some(id.clone()))
                }
                Some(id) => launch.with_session(id.clone()),
                None => launch,
            };
            registry.start_agent(work_id, preset, Some(agent.role.clone()), &launch)?;
        }
        Ok(registry
            .workspace(work_id)
            .map(|w| w.agents().to_vec())
            .unwrap_or_default())
    }

    /// Undoes a start that failed: closes the workspace if it was opened
    /// and deletes the work's folder and index entry.
    fn forget_work(&self, id: WorkspaceId, work: Option<WorkDb>) {
        // Not open if starting failed before the team was.
        let _ = self.registry().remove_workspace(id);
        if let (Some(store), Some(work)) = (&self.store, work)
            && let Err(error) = store.discard_work(work)
        {
            tracing::warn!(%id, error = error_chain(&error), "can't remove a work that failed to start");
        }
    }

    /// Saved work, most recently opened first.
    fn saved_works(&self, limit: usize) -> Vec<RecentWorkInfo> {
        let Some(store) = &self.store else {
            return Vec::new();
        };
        let works = store.index().recent(limit).unwrap_or_else(|error| {
            tracing::error!(error = error_chain(&error), "can't list saved work");
            Vec::new()
        });
        works
            .into_iter()
            .map(|w| RecentWorkInfo {
                id: w.id,
                name: w.name,
                kind: w.kind.and_then(|k| WorkKind::try_from(k).ok()),
                folder: w.folder,
            })
            .collect()
    }

    /// Whether `preset`'s program saved session `id`, where its adapter
    /// says it keeps sessions; assumed when it can't be checked.
    fn session_saved(&self, registry: &Registry, preset: &PresetId, id: &str) -> bool {
        registry
            .preset(preset)
            .and_then(|p| p.adapter().sessions())
            .is_none_or(|store| self.sessions.saved(store, id))
    }

    /// Folders agents worked in, most used first; repos follow from the
    /// background scan.
    fn frecent_folders(&self) -> Vec<String> {
        let Some(store) = &self.store else {
            return Vec::new();
        };
        store.index().frecent_folders(20).unwrap_or_else(|error| {
            tracing::error!(error = error_chain(&error), "can't rank recent folders");
            Vec::new()
        })
    }

    /// Saves answers and voice notes, and writes `brief.md` for template
    /// work. Returns the brief's path, or `None` when there's no brief.
    fn save_brief(
        work: &WorkDb,
        request: &NewWorkRequestInfo,
        kind: Option<core::WorkKind>,
        folder: &str,
    ) -> Result<Option<std::path::PathBuf>, CoreError> {
        let mut notes = Vec::new();
        for clip in &request.voice {
            let audio = Self::save_clip(work, clip)?;
            if clip.question.is_none() {
                work.add_note(None, &clip.transcript, Some(&audio))?;
                notes.push(BriefNote::spoken(clip.transcript.clone()));
            }
        }
        let answers: Vec<(String, String)> = request
            .answers
            .iter()
            .filter(|a| !a.answer.trim().is_empty())
            .map(|a| (a.question.clone(), a.answer.clone()))
            .collect();
        for (question, answer) in &answers {
            work.add_note(Some(question), answer, None)?;
        }
        if request.template.is_none() && answers.is_empty() && notes.is_empty() {
            return Ok(None);
        }
        let markdown = Brief::new(&request.name)
            .with_kind(kind)
            .with_folder(folder)
            .read_only(request.read_only)
            .with_answers(&answers)
            .with_notes(&notes)
            .render();
        Ok(Some(work.write_brief(&markdown)?))
    }

    /// Moves a recording into the work and saves its transcript; returns the
    /// audio id.
    fn save_clip(work: &WorkDb, clip: &VoiceClipInfo) -> Result<String, CoreError> {
        let audio = work.add_audio(
            std::path::Path::new(&clip.recording),
            "audio/mp4",
            clip.duration_ms,
        )?;
        work.add_transcript(
            &audio.id,
            &clip.provider,
            clip.language.as_deref(),
            &clip.transcript,
        )?;
        Ok(audio.id)
    }

    fn save_agents(work: &WorkDb, agents: &[core::Agent]) -> Result<(), CoreError> {
        for (position, agent) in agents.iter().enumerate() {
            Self::save_agent(work, position, agent)?;
        }
        Ok(())
    }

    /// Saves one agent at `position` (0 is the lead).
    pub(crate) fn save_agent(
        work: &WorkDb,
        position: usize,
        agent: &core::Agent,
    ) -> Result<(), CoreError> {
        let row = AgentRow::new(
            agent.id(),
            u32::try_from(position).unwrap_or(u32::MAX),
            agent.name(),
            agent.role().clone(),
        )
        .started_with(
            agent.preset().cloned(),
            agent.command().map(str::to_owned),
            agent.session_id().map(str::to_owned),
        )
        .with_permission(agent.permission());
        work.add_agent(&row)?;
        Ok(())
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{CoreEventSink, StorageMode};

    #[derive(Default)]
    struct Sink(Mutex<Vec<CoreEvent>>);

    impl CoreEventSink for Sink {
        fn on_event(&self, event: CoreEvent) {
            self.0.lock().unwrap().push(event);
        }
    }

    /// A core saving under `root`, with Claude's sessions in
    /// `root/claude/projects`.
    fn core(root: &std::path::Path) -> Arc<Core> {
        Core::build(
            Arc::new(Sink::default()),
            None,
            StorageMode::At {
                root: root.display().to_string(),
            },
            makima_store::Sessions::at(Some(claude_projects(root))),
        )
    }

    fn claude_projects(root: &std::path::Path) -> std::path::PathBuf {
        root.join("claude/projects")
    }

    /// What Claude does after an agent's first message: save its session.
    fn claude_saves(root: &std::path::Path, session: &str) {
        let folder = claude_projects(root).join("-tmp-app");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(format!("{session}.jsonl")), "{}").unwrap();
    }

    fn fix_request(folder: &str, recording: String) -> NewWorkRequestInfo {
        NewWorkRequestInfo {
            name: "fix login".into(),
            folder: folder.into(),
            template: Some("fix".into()),
            kind: Some(WorkKind::Fix),
            lead: "claude".into(),
            children: vec![ChildRequestInfo {
                preset: "claude".into(),
                role: AgentRole::Tester,
            }],
            read_only: false,
            permission: None,
            answers: vec![
                BriefAnswer {
                    question: "What's broken?".into(),
                    answer: "login loops".into(),
                },
                BriefAnswer {
                    question: "Expected instead".into(),
                    answer: String::new(),
                },
            ],
            voice: vec![VoiceClipInfo {
                recording,
                duration_ms: Some(1500),
                transcript: "only on Safari".into(),
                question: None,
                provider: "apple".into(),
                language: Some("en-US".into()),
            }],
        }
    }

    #[test]
    fn starting_work_saves_it_and_briefs_the_team() {
        let root = tempfile::tempdir().unwrap();
        let core = core(root.path());
        let recording = std::path::PathBuf::from(core.recordings_dir().unwrap()).join("clip.m4a");
        std::fs::write(&recording, b"audio").unwrap();

        let info = core
            .start_work(fix_request("/tmp/app", recording.display().to_string()))
            .unwrap();

        assert_eq!(info.folder.as_deref(), Some("/tmp/app"));
        let lead = &info.agents[0];
        let session = lead.session_id.clone().unwrap();
        let command = lead.command.clone().unwrap();
        assert!(
            command.starts_with(&format!("claude --session-id {session} 'You are the lead")),
            "{command}"
        );
        assert!(
            info.agents[1]
                .command
                .as_deref()
                .unwrap()
                .contains("You are child 1, the tester")
        );

        let work = core.works().get(&info.id).cloned().unwrap();
        let brief = std::fs::read_to_string(work.brief_path()).unwrap();
        assert!(brief.contains("## What's broken?\n\nlogin loops"));
        assert!(brief.contains("only on Safari (spoken)"));
        assert!(!brief.contains("Expected instead"));
        assert_eq!(work.agents().unwrap().len(), 2);
        assert!(!recording.exists(), "the recording moved into the work");

        core.record_message(info.id, "summarise", vec![lead.id], None)
            .unwrap();
        assert_eq!(work.messages().unwrap()[0].recipients, [lead.id]);
        assert_eq!(core.saved_works(5)[0].name, "fix login");
    }

    #[test]
    fn reopening_resumes_claude_sessions() {
        let root = tempfile::tempdir().unwrap();
        let first = core(root.path());
        let recording = std::path::PathBuf::from(first.recordings_dir().unwrap()).join("clip.m4a");
        std::fs::write(&recording, b"audio").unwrap();
        let info = first
            .start_work(fix_request("/tmp/app", recording.display().to_string()))
            .unwrap();
        let session = info.agents[0].session_id.clone().unwrap();
        let unsent = info.agents[1].session_id.clone().unwrap();
        // Only the lead got a message, so only its session was saved.
        claude_saves(root.path(), &session);

        // A new app launch: a fresh core over the same storage.
        let second = core(root.path());
        let reopened = second.reopen_work(info.id).unwrap();
        assert_eq!(reopened.id, info.id);
        assert_eq!(reopened.agents.len(), 2);
        assert_eq!(
            reopened.agents[0].command.as_deref(),
            Some(format!("claude --resume {session}").as_str())
        );
        assert_eq!(
            reopened.agents[1].command.as_deref(),
            Some(format!("claude --session-id {unsent}").as_str()),
            "a session Claude never saved starts fresh, same id"
        );
        assert_eq!(
            second.reopen_work(info.id).unwrap().agents.len(),
            2,
            "already open"
        );
    }

    #[test]
    fn a_failed_start_leaves_nothing_behind() {
        let root = tempfile::tempdir().unwrap();
        let core = core(root.path());
        let mut request = fix_request("/tmp/app", String::new());
        request.voice.clear();
        request.children[0].preset = "nope".into();

        assert!(matches!(
            core.start_work(request),
            Err(CoreError::UnknownPreset { id }) if id == "nope"
        ));
        assert!(core.workspaces().is_empty());
        assert!(core.saved_works(5).is_empty());
        let works_dir = &core.store.as_ref().unwrap().paths().works_dir;
        assert_eq!(std::fs::read_dir(works_dir).unwrap().count(), 0);
    }

    #[test]
    fn repeated_roles_are_counted_in_the_team() {
        let mut request = fix_request("/tmp/app", String::new());
        request.children = (0..6)
            .map(|_| ChildRequestInfo {
                preset: "claude".into(),
                role: AgentRole::Implementer,
            })
            .collect();
        assert_eq!(
            team_summary(&request),
            "lead (claude) + 6 × implementer (claude)"
        );
    }

    #[test]
    fn template_agents_are_told_their_job() {
        let root = tempfile::tempdir().unwrap();
        let core = core(root.path());
        let recording = std::path::PathBuf::from(core.recordings_dir().unwrap()).join("clip.m4a");
        std::fs::write(&recording, b"audio").unwrap();
        let info = core
            .start_work(fix_request("/tmp/app", recording.display().to_string()))
            .unwrap();
        let lead = info.agents[0].command.clone().unwrap();
        assert!(lead.contains("fixing a bug in /tmp/app"), "{lead}");
        assert!(lead.contains("Reproduce the bug first"), "{lead}");
        assert!(lead.contains("lead (claude) + tester (claude)"), "{lead}");
        assert!(lead.contains("plan.md"), "{lead}");
        let tester = info.agents[1].command.clone().unwrap();
        assert!(tester.contains("You are child 1, the tester"), "{tester}");
        assert!(tester.contains("Write and run tests"), "{tester}");
        assert!(tester.contains("ready"), "{tester}");
    }

    #[test]
    fn quick_start_has_no_brief() {
        let root = tempfile::tempdir().unwrap();
        let core = core(root.path());
        let info = core
            .start_work(NewWorkRequestInfo {
                name: "makima".into(),
                folder: "/tmp/makima".into(),
                template: None,
                kind: None,
                lead: "claude".into(),
                children: Vec::new(),
                read_only: false,
                permission: None,
                answers: Vec::new(),
                voice: Vec::new(),
            })
            .unwrap();
        let command = info.agents[0].command.clone().unwrap();
        assert!(
            command.starts_with("claude --session-id ") && !command.contains('\''),
            "{command}"
        );
        assert!(!core.works().get(&info.id).unwrap().brief_path().exists());
    }

    #[test]
    fn the_flow_offers_templates_and_starts_quick_work() {
        let root = tempfile::tempdir().unwrap();
        let core = core(root.path());
        let flow = core.new_work_flow(false);
        let events: Vec<_> = [b"\r" as &[u8], b"\r"]
            .iter()
            .flat_map(|k| flow.feed(k))
            .collect();
        assert!(
            matches!(&events[..], [FlowEventInfo::Start { request }] if request.lead == "claude")
        );
    }

    #[test]
    fn reopening_unknown_work_is_a_typed_error() {
        let root = tempfile::tempdir().unwrap();
        let core = core(root.path());
        let id = WorkspaceId::new();
        assert!(matches!(
            core.reopen_work(id),
            Err(CoreError::UnknownWork { id: missing }) if missing == id
        ));
        let off = Core::new(Arc::new(Sink::default()), None, StorageMode::Off);
        assert!(matches!(off.reopen_work(id), Err(CoreError::StorageOff)));
    }

    #[test]
    fn the_works_permission_mode_reaches_every_agent_and_survives_reopening() {
        let root = tempfile::tempdir().unwrap();
        let first = core(root.path());
        let recording = std::path::PathBuf::from(first.recordings_dir().unwrap()).join("clip.m4a");
        std::fs::write(&recording, b"audio").unwrap();
        let mut request = fix_request("/tmp/app", recording.display().to_string());
        request.permission = Some(PermissionMode::Bypass);
        let info = first.start_work(request).unwrap();
        for agent in &info.agents {
            assert!(
                agent
                    .command
                    .as_deref()
                    .unwrap()
                    .contains("--dangerously-skip-permissions")
            );
            assert_eq!(agent.permission, Some(PermissionMode::Bypass));
        }
        let child = first.add_preset_agent(info.id, "codex".into()).unwrap();
        assert!(
            child
                .command
                .as_deref()
                .unwrap()
                .contains("--dangerously-bypass-approvals-and-sandbox"),
            "new children follow the work's mode"
        );

        let second = core(root.path());
        let reopened = second.reopen_work(info.id).unwrap();
        assert_eq!(reopened.agents.len(), 3, "the added child was saved too");
        assert!(
            reopened.agents[0]
                .command
                .as_deref()
                .unwrap()
                .ends_with("--dangerously-skip-permissions")
        );
    }

    #[test]
    fn scanned_repos_reach_the_folder_picker() {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("code/client/app");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let config = root.path().join("config.toml");
        std::fs::write(
            &config,
            format!(
                "[work]\nroots = [{:?}]\n",
                root.path().join("code").display().to_string()
            ),
        )
        .unwrap();
        let core = Core::new(
            Arc::new(Sink::default()),
            Some(config.display().to_string()),
            StorageMode::Off,
        );
        let flow = core.new_work_flow(false);
        assert!(core.repos.wait(std::time::Duration::from_secs(5)).is_some());

        // Quick start, up to the folder (past permits, children, agent,
        // name), pick "app", then down to start.
        let (up, down) = (b"\x1b[A" as &[u8], b"\t" as &[u8]);
        let keys = [
            b"\r", up, up, up, up, up, b"\r", b"app", b"\r", down, down, down, down, down, b"\r",
        ];
        let events: Vec<_> = keys.iter().flat_map(|k| flow.feed(k)).collect();
        assert!(
            matches!(&events[..], [FlowEventInfo::Start { request }] if request.folder == repo.display().to_string()),
            "{events:?}"
        );
    }
}
