//! Starting work, without a command language: a welcome screen (Quick
//! start, From a template, Build your own, and recent work), a template
//! picker, a short form whose every field has a default, and a fuzzy
//! folder picker.
//!
//! Deterministic: fields come in a fixed order and choices from fixed
//! lists, so the same keys always do the same thing. Enter twice from the
//! welcome screen starts a quick session.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph, Widget};

use makima_core::{
    AgentPreset, AgentRole, DEFAULT_LEAD, PermissionMode, PresetId, Template, WorkKind, WorkspaceId,
};

use crate::editor::LineEditor;
use crate::form::{
    Action, Button, Field, Form, FormEvent, Group, Number, Select, TextInput, Theme, Value,
};
use crate::fuzzy;
use crate::input::{Key, parse_keys};
use crate::render::{ACCENT, MUTED, draw_logo};

/// Work opened before, to reopen with one key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct RecentWork {
    pub id: WorkspaceId,
    pub name: String,
    pub kind: Option<WorkKind>,
    pub folder: Option<String>,
}

impl RecentWork {
    #[must_use]
    pub fn new(
        id: WorkspaceId,
        name: impl Into<String>,
        kind: Option<WorkKind>,
        folder: Option<String>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            folder,
        }
    }
}

/// Everything the flow offers. Build with [`FlowOptions::new`] and the
/// `with_*` methods.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FlowOptions {
    pub presets: Vec<AgentPreset>,
    pub templates: Vec<Template>,
    pub recents: Vec<RecentWork>,
    /// Folder picker candidates (recent folders, then discovered repos).
    pub folders: Vec<String>,
    pub default_folder: String,
    /// The lead preset quick starts and template forms begin with.
    pub default_preset: PresetId,
    /// The home directory, shown as `~`.
    pub home: String,
    /// Esc on the welcome screen closes it (there's work to go back to).
    pub can_close: bool,
    /// `[permissions] default`, shown as what "from config" means.
    pub config_permission: Option<PermissionMode>,
    /// Helix-style forms: `j`/`k` move, `i` edits, Esc stops editing.
    pub modal: bool,
}

impl FlowOptions {
    /// Offers `presets` and `templates`; the first folder candidate (or
    /// home) is the default folder and [`DEFAULT_LEAD`] (or the first
    /// preset) the default agent.
    #[must_use]
    pub fn new(presets: Vec<AgentPreset>, templates: Vec<Template>) -> Self {
        let default_preset = PresetId::from(DEFAULT_LEAD);
        Self {
            presets,
            templates,
            recents: Vec::new(),
            folders: Vec::new(),
            default_folder: String::new(),
            default_preset,
            home: String::new(),
            can_close: false,
            config_permission: None,
            modal: false,
        }
    }

    #[must_use]
    pub fn with_recents(mut self, recents: Vec<RecentWork>) -> Self {
        self.recents = recents;
        self
    }

    /// Folder candidates; the first is the default folder.
    #[must_use]
    pub fn with_folders(mut self, folders: Vec<String>) -> Self {
        let mut unique: Vec<String> = Vec::new();
        for folder in folders.into_iter().map(|f| normalize_path(&f)) {
            if !unique.contains(&folder) {
                unique.push(folder);
            }
        }
        if let Some(first) = unique.first() {
            self.default_folder.clone_from(first);
        }
        self.folders = unique;
        self
    }

    /// The home directory, shown as `~`, and the default folder when there
    /// are no candidates (in either call order).
    /// Starts the lead on `preset` (the config's `[work] lead`) when it's
    /// offered; the first preset otherwise.
    #[must_use]
    pub fn with_default_preset(mut self, preset: PresetId) -> Self {
        self.default_preset = preset;
        self
    }

    #[must_use]
    pub fn with_home(mut self, home: impl Into<String>) -> Self {
        self.home = normalize_path(&home.into());
        if self.folders.is_empty() {
            self.default_folder.clone_from(&self.home);
        }
        self
    }

    #[must_use]
    pub fn closable(mut self, can_close: bool) -> Self {
        self.can_close = can_close;
        self
    }

    /// Helix-style forms (see [`Form::modal`]).
    #[must_use]
    pub fn modal(mut self, modal: bool) -> Self {
        self.modal = modal;
        self
    }

    /// The config's default permission mode, for the form's "from config".
    #[must_use]
    pub fn with_config_permission(mut self, mode: Option<PermissionMode>) -> Self {
        self.config_permission = mode;
        self
    }
}

/// A voice note recorded while filling the form.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct VoiceClip {
    /// The recording file, moved into the work when it starts.
    pub recording: String,
    pub duration_ms: Option<u64>,
    pub transcript: String,
    /// The brief question it was spoken into; `None` for a free note.
    pub question: Option<String>,
    /// Which speech-to-text provider transcribed it, e.g. `apple`.
    pub provider: String,
    pub language: Option<String>,
}

impl VoiceClip {
    #[must_use]
    pub fn new(
        recording: impl Into<String>,
        transcript: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        Self {
            recording: recording.into(),
            duration_ms: None,
            transcript: transcript.into(),
            question: None,
            provider: provider.into(),
            language: None,
        }
    }

    #[must_use]
    pub fn with_duration_ms(mut self, duration_ms: Option<u64>) -> Self {
        self.duration_ms = duration_ms;
        self
    }

    #[must_use]
    pub fn with_language(mut self, language: Option<String>) -> Self {
        self.language = language;
        self
    }

    /// The brief question it answers.
    #[must_use]
    pub fn for_question(mut self, question: Option<String>) -> Self {
        self.question = question;
        self
    }
}

/// One child to start.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ChildRequest {
    pub preset: PresetId,
    pub role: AgentRole,
}

/// Everything needed to start a work.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct NewWorkRequest {
    pub name: String,
    pub folder: String,
    pub template: Option<String>,
    pub kind: Option<WorkKind>,
    pub lead: PresetId,
    pub children: Vec<ChildRequest>,
    pub read_only: bool,
    /// The whole team's permission mode; `None` resolves per agent from
    /// the config.
    pub permission: Option<PermissionMode>,
    /// Question and answer pairs.
    pub answers: Vec<(String, String)>,
    pub voice: Vec<VoiceClip>,
}

/// What the flow wants the host to do.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FlowEvent {
    Start(NewWorkRequest),
    /// Reopen a recent work.
    Reopen(WorkspaceId),
    /// Leave the flow (back to running work).
    Close,
}

/// The new-work form: a single page whose fields come from the template.
#[derive(Debug)]
struct WorkForm {
    template: Option<usize>,
    form: Form,
    /// The name was typed, so it no longer follows the folder.
    name_edited: bool,
    voice: Vec<VoiceClip>,
}

#[derive(Debug)]
enum Screen {
    Home,
    Templates,
    Form(Box<WorkForm>),
    Picker {
        form: Box<WorkForm>,
        query: LineEditor,
    },
}

/// The new-work flow's state.
#[derive(Debug)]
pub struct NewWorkFlow {
    options: FlowOptions,
    screen: Screen,
    /// The highlighted row in the current list (home, templates, picker).
    selected: usize,
    notice: Option<String>,
    /// "● recording", "transcribing …"; shown above the hints.
    voice_status: Option<String>,
}

/// Rows on the welcome screen before the recent works.
const HOME_OPTIONS: usize = 3;
const MAX_RECENTS: usize = 9;

impl NewWorkFlow {
    #[must_use]
    pub fn new(options: FlowOptions) -> Self {
        Self {
            options,
            screen: Screen::Home,
            selected: 0,
            notice: None,
            voice_status: None,
        }
    }

    /// Applies one chunk of terminal input.
    pub fn feed(&mut self, input: &[u8]) -> Vec<FlowEvent> {
        parse_keys(input)
            .iter()
            .filter_map(|key| self.handle(key))
            .collect()
    }

    /// Applies one key.
    pub fn handle(&mut self, key: &Key) -> Option<FlowEvent> {
        self.notice = None;
        match std::mem::replace(&mut self.screen, Screen::Home) {
            Screen::Home => self.home_key(key),
            Screen::Templates => self.templates_key(key),
            Screen::Form(form) => self.form_key(*form, key),
            Screen::Picker { form, query } => self.picker_key(*form, query, key),
        }
    }

    /// Shows a one-line message (e.g. why starting failed).
    pub fn set_notice(&mut self, notice: Option<String>) {
        self.notice = notice;
    }

    /// Shows a recording/transcribing indicator, or clears it.
    pub fn set_voice_status(&mut self, status: Option<String>) {
        self.voice_status = status;
    }

    /// Adds a transcribed voice note: into the focused question if there is
    /// one, otherwise as a free note.
    pub fn add_voice(&mut self, mut clip: VoiceClip) {
        let Screen::Form(form) = &mut self.screen else {
            self.notice = Some("open a form to add voice notes".into());
            return;
        };
        let focused = form.form.focused().to_owned();
        if let Some(index) = question_index(&focused)
            && form
                .form
                .field_mut(&focused)
                .is_some_and(|f| f.append_text(&clip.transcript))
        {
            clip.question = form
                .template
                .and_then(|t| self.options.templates.get(t))
                .and_then(|t| t.questions.get(index))
                .map(|q| q.label.clone());
        }
        form.voice.push(clip);
    }

    /// Adds folder picker candidates (e.g. repos found after the flow
    /// opened), after the ones it has; duplicates are dropped.
    pub fn add_folders(&mut self, folders: impl IntoIterator<Item = String>) {
        for folder in folders.into_iter().map(|f| normalize_path(&f)) {
            if !self.options.folders.contains(&folder) {
                self.options.folders.push(folder);
            }
        }
    }

    fn template<'a>(&'a self, form: &WorkForm) -> Option<&'a Template> {
        form.template.and_then(|i| self.options.templates.get(i))
    }

    fn recents(&self) -> &[RecentWork] {
        let n = self.options.recents.len().min(MAX_RECENTS);
        &self.options.recents[..n]
    }

    // MARK: - Home

    fn home_key(&mut self, key: &Key) -> Option<FlowEvent> {
        let rows = HOME_OPTIONS + self.recents().len();
        match key {
            Key::Up | Key::Char('k') => self.selected = (self.selected + rows - 1) % rows,
            Key::Down | Key::Char('j') | Key::Tab => self.selected = (self.selected + 1) % rows,
            Key::Char('q') => return self.open_home_row(0),
            Key::Char('t') => return self.open_home_row(1),
            Key::Char(c @ '1'..='9') => {
                let index = c.to_digit(10).map_or(0, |d| d as usize) - 1;
                if let Some(work) = self.recents().get(index) {
                    return Some(FlowEvent::Reopen(work.id));
                }
            }
            Key::Enter => return self.open_home_row(self.selected),
            Key::Esc if self.options.can_close => return Some(FlowEvent::Close),
            _ => {}
        }
        None
    }

    fn open_home_row(&mut self, row: usize) -> Option<FlowEvent> {
        match row {
            0 => self.screen = Screen::Form(Box::new(self.new_form(None))),
            1 => {
                self.screen = Screen::Templates;
                self.selected = 0;
            }
            2 => self.notice = Some("building your own templates is coming soon".into()),
            _ => {
                return self
                    .recents()
                    .get(row - HOME_OPTIONS)
                    .map(|w| FlowEvent::Reopen(w.id));
            }
        }
        None
    }

    // MARK: - Templates

    fn templates_key(&mut self, key: &Key) -> Option<FlowEvent> {
        self.screen = Screen::Templates;
        let rows = self.options.templates.len().max(1);
        match key {
            Key::Up | Key::Char('k') => self.selected = (self.selected + rows - 1) % rows,
            Key::Down | Key::Char('j') | Key::Tab => self.selected = (self.selected + 1) % rows,
            Key::Char(c @ '1'..='9') => {
                let index = c.to_digit(10).map_or(0, |d| d as usize) - 1;
                if index < self.options.templates.len() {
                    self.screen = Screen::Form(Box::new(self.new_form(Some(index))));
                }
            }
            Key::Enter if !self.options.templates.is_empty() => {
                self.screen = Screen::Form(Box::new(self.new_form(Some(self.selected))));
            }
            Key::Esc => {
                self.screen = Screen::Home;
                self.selected = 1;
            }
            _ => {}
        }
        None
    }

    // MARK: - Form

    /// The single-page form: where, team, brief (for templates), start.
    fn new_form(&self, template: Option<usize>) -> WorkForm {
        let t = template.and_then(|i| self.options.templates.get(i));
        let presets = &self.options.presets;
        let preset_index = |id: &PresetId| presets.iter().position(|p| p.id() == id).unwrap_or(0);
        let labels: Vec<String> = presets.iter().map(|p| p.label().to_owned()).collect();
        let lead = preset_index(t.map_or(&self.options.default_preset, |t| &t.lead));
        let children = t.map_or(0, |t| u32::try_from(t.children.len()).unwrap_or(u32::MAX));

        let folder = &self.options.default_folder;
        let mut team: Vec<Box<dyn Field>> = vec![
            Box::new(
                Select::new(
                    LEAD,
                    if t.is_some() { "lead" } else { "agent" },
                    labels.clone(),
                )
                .with_selected(lead)
                .with_description("the agent you talk to by default"),
            ),
            Box::new(
                Number::new(CHILDREN, "children", children)
                    .with_description("how many to start with; add more any time while it runs"),
            ),
            Box::new(
                Select::new(
                    CHILD,
                    "they run",
                    std::iter::once("same as lead".to_owned())
                        .chain(labels)
                        .collect(),
                )
                .with_description("what the children run"),
            ),
        ];
        let (permission_options, permission_descriptions) = self.permission_choices();
        let selected_permission = t
            .and_then(|t| t.permission)
            .and_then(|m| PermissionMode::ALL.iter().position(|p| *p == m))
            .map_or(0, |i| i + 1);
        team.push(Box::new(
            Select::new(PERMITS, "permits", permission_options)
                .with_option_descriptions(permission_descriptions)
                .with_selected(selected_permission),
        ));
        let mut groups = vec![
            Group::new(
                "where",
                vec![
                    Box::new(
                        Action::new(FOLDER, "folder", self.abbreviate(folder), "choose")
                            .with_description("the folder the agents work in"),
                    ),
                    Box::new(
                        TextInput::new(NAME, "name")
                            .with_text(&folder_name(folder))
                            .with_description("shown in the tabs; follows the folder until edited"),
                    ),
                ],
            ),
            Group::new("team", team),
        ];
        if let Some(t) = t.filter(|t| !t.questions.is_empty()) {
            groups.push(Group::new(
                "brief",
                t.questions
                    .iter()
                    .enumerate()
                    .map(|(i, q)| {
                        Box::new(
                            TextInput::new(question_key(i), q.label.clone())
                                .with_placeholder(q.hint.clone())
                                .stacked(),
                        ) as Box<dyn Field>
                    })
                    .collect(),
            ));
        }
        groups.push(Group::new(
            "",
            vec![Box::new(Button::new(START, "start").with_description(
                "type to name it · ↑ to change anything else",
            ))],
        ));
        let mut form = Form::new(groups)
            .typing_into(NAME)
            .modal(self.options.modal);
        form.set_hidden(CHILD, children == 0);
        // Quick start: Enter starts. Templates start at the top, so the
        // whole team and brief can be set before starting.
        form.focus_on(if t.is_some() { FOLDER } else { START });
        WorkForm {
            template,
            form,
            name_edited: false,
            voice: Vec::new(),
        }
    }

    /// "From config" (or the program's own default) and every mode, with a
    /// description each.
    fn permission_choices(&self) -> (Vec<String>, Vec<String>) {
        let first = match self.options.config_permission {
            Some(mode) => (
                format!("config ({mode})"),
                format!("from [permissions]: {}", mode.description()),
            ),
            None => (
                "default".to_owned(),
                "each program's own default".to_owned(),
            ),
        };
        std::iter::once(first)
            .chain(
                PermissionMode::ALL
                    .iter()
                    .map(|m| (m.id().to_owned(), m.description().to_owned())),
            )
            .unzip()
    }

    fn form_key(&mut self, mut form: WorkForm, key: &Key) -> Option<FlowEvent> {
        // A modal form uses Esc itself (to stop editing) and says when to go.
        if *key == Key::Esc && !form.form.is_modal() {
            self.selected = form.template.unwrap_or(0);
            self.screen = if form.template.is_some() {
                Screen::Templates
            } else {
                Screen::Home
            };
            return None;
        }
        let name_before = form.form.value(NAME);
        let event = form.form.handle(key);
        if form.form.value(NAME) != name_before {
            form.name_edited = true;
        }
        let children = form.form.value(CHILDREN) != Value::Number(0);
        form.form.set_hidden(CHILD, !children);
        match event {
            Some(FormEvent::Back) => {
                self.selected = form.template.unwrap_or(0);
                self.screen = if form.template.is_some() {
                    Screen::Templates
                } else {
                    Screen::Home
                };
                return None;
            }
            Some(FormEvent::Activated(field)) if field == FOLDER => {
                self.selected = 0;
                self.screen = Screen::Picker {
                    form: Box::new(form),
                    query: LineEditor::new(),
                };
                return None;
            }
            Some(FormEvent::Activated(field)) if field == START => match self.request(&form) {
                Ok(request) => {
                    self.screen = Screen::Form(Box::new(form));
                    return Some(FlowEvent::Start(request));
                }
                Err((field, message)) => form.form.set_error(field, message),
            },
            _ => {}
        }
        self.screen = Screen::Form(Box::new(form));
        None
    }

    /// The request, or the field that's wrong and why.
    fn request(&self, form: &WorkForm) -> Result<NewWorkRequest, (&'static str, String)> {
        let text = |key: &str| match form.form.value(key) {
            Value::Text(text) => text,
            _ => String::new(),
        };
        let choice = |key: &str| match form.form.value(key) {
            Value::Choice(i) => i,
            _ => 0,
        };
        let name = text(NAME).trim().to_owned();
        if name.is_empty() {
            return Err((NAME, "give the work a name".into()));
        }
        // Agents start from the brief: an empty one only gets "nothing to
        // do yet" from every one of them.
        if self.template(form).is_some_and(|t| !t.questions.is_empty())
            && text(&question_key(0)).trim().is_empty()
        {
            return Err((
                FIRST_QUESTION,
                "answer this first; the team starts from it".into(),
            ));
        }
        let folder = self.expand(&text(FOLDER));
        if folder.trim().is_empty() {
            return Err((FOLDER, "choose a folder".into()));
        }
        let preset_id = |i: usize| {
            self.options
                .presets
                .get(i)
                .map_or_else(|| self.options.default_preset.clone(), |p| p.id().clone())
        };
        let lead = preset_id(choice(LEAD));
        let child_preset = choice(CHILD).checked_sub(1).map(preset_id);
        let count = match form.form.value(CHILDREN) {
            Value::Number(n) => usize::try_from(n).unwrap_or(usize::MAX),
            _ => 0,
        };
        let template = self.template(form);
        let members = template.map_or(&[][..], |t| t.children.as_slice());
        let children = (0..count)
            .map(|i| {
                let member = members.get(i);
                let preset = child_preset
                    .clone()
                    .or_else(|| member.and_then(|m| m.preset.clone()))
                    .unwrap_or_else(|| lead.clone());
                let role = member.map_or(AgentRole::Implementer, |m| m.role.clone());
                ChildRequest { preset, role }
            })
            .collect();
        let answers = template
            .map(|t| {
                t.questions
                    .iter()
                    .enumerate()
                    .map(|(i, q)| (q.label.clone(), text(&question_key(i))))
                    .collect()
            })
            .unwrap_or_default();
        Ok(NewWorkRequest {
            name,
            folder,
            template: template.map(|t| t.id.clone()),
            kind: template.and_then(|t| t.kind),
            lead,
            children,
            read_only: template.is_some_and(|t| t.read_only),
            permission: choice(PERMITS)
                .checked_sub(1)
                .and_then(|i| PermissionMode::ALL.get(i).copied()),
            answers,
            voice: form.voice.clone(),
        })
    }

    // MARK: - Folder picker

    fn picker_candidates(&self, query: &str) -> Vec<String> {
        let typed = query.trim();
        let mut candidates = Vec::new();
        if typed.starts_with('/') || typed.starts_with('~') {
            candidates.push(typed.to_owned());
        }
        let shown: Vec<String> = self
            .options
            .folders
            .iter()
            .map(|f| self.abbreviate(f))
            .collect();
        candidates.extend(
            fuzzy::filter(query, &shown)
                .into_iter()
                .map(|i| self.options.folders[i].clone()),
        );
        candidates
    }

    fn picker_key(
        &mut self,
        mut form: WorkForm,
        mut query: LineEditor,
        key: &Key,
    ) -> Option<FlowEvent> {
        let candidates = self.picker_candidates(&query.text());
        match key {
            Key::Esc => {
                self.screen = Screen::Form(Box::new(form));
                return None;
            }
            Key::Up => self.selected = self.selected.saturating_sub(1),
            Key::Down | Key::Tab => {
                self.selected = (self.selected + 1).min(candidates.len().saturating_sub(1));
            }
            Key::Enter => {
                if let Some(folder) = candidates.get(self.selected) {
                    let shown = self.abbreviate(folder);
                    if let Some(field) = form.form.field_mut(FOLDER) {
                        field.set_text(&shown);
                    }
                    if !form.name_edited
                        && let Some(field) = form.form.field_mut(NAME)
                    {
                        field.set_text(&folder_name(folder));
                    }
                }
                self.screen = Screen::Form(Box::new(form));
                return None;
            }
            key => {
                query.handle(key);
                self.selected = 0;
            }
        }
        self.screen = Screen::Picker {
            form: Box::new(form),
            query,
        };
        None
    }

    // MARK: - Drawing

    /// Draws the flow in `body`; returns the cursor when a text field has
    /// focus.
    pub fn draw(&self, body: Rect, buffer: &mut Buffer) -> Option<(u16, u16)> {
        // Menus centre on what they show and may be wide; forms keep a
        // fixed column so typing doesn't move them.
        let fit = matches!(self.screen, Screen::Home | Screen::Templates);
        let mut canvas = Canvas {
            // Forms are cards a fixed 64 columns inside, so typing never
            // moves them.
            width: match &self.screen {
                Screen::Home | Screen::Templates => 120,
                // Template forms carry the brief: give its answers room.
                Screen::Form(form) if form.template.is_some() => 84,
                _ => 64,
            }
            .min(body.width.saturating_sub(12)),
            fit,
            card: None,
            body,
            lines: Vec::new(),
            cursor: None,
        };
        match &self.screen {
            Screen::Home => self.draw_home(&mut canvas),
            Screen::Templates => self.draw_templates(&mut canvas),
            Screen::Form(form) => self.draw_form(form, &mut canvas),
            Screen::Picker { form, query } => self.draw_picker(form, query, &mut canvas),
        }
        if let Some(status) = &self.voice_status {
            canvas.blank();
            canvas.push(Line::from(Span::styled(
                status.clone(),
                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
        }
        if let Some(notice) = &self.notice {
            canvas.blank();
            canvas.push(Line::from(Span::styled(
                notice.clone(),
                Style::new().fg(Color::Yellow),
            )));
        }
        canvas.render(buffer)
    }

    fn draw_home(&self, canvas: &mut Canvas) {
        canvas.logo();
        canvas.push(Line::from(Span::styled(
            "run teams of coding agents side by side",
            dim(),
        )));
        canvas.blank();
        let kinds: Vec<&str> = self
            .options
            .templates
            .iter()
            .filter_map(|t| t.kind.map(WorkKind::id))
            .collect();
        let rows: [(&str, String, bool); HOME_OPTIONS] = [
            (
                "Quick start",
                "one agent in a folder · ⏎ ⏎ to go".into(),
                true,
            ),
            ("From a template", kinds.join(" · "), true),
            ("Build your own", "coming soon".into(), false),
        ];
        for (i, (label, detail, enabled)) in rows.iter().enumerate() {
            canvas.push(choice_row(label, detail, self.selected == i, *enabled, 18));
        }
        if !self.recents().is_empty() {
            canvas.blank();
            canvas.push(Line::from(Span::styled(
                "RECENT",
                dim().add_modifier(Modifier::BOLD),
            )));
            for (i, work) in self.recents().iter().enumerate() {
                let detail = format!(
                    "{:<9}{}",
                    work.kind.map(WorkKind::id).unwrap_or_default(),
                    work.folder
                        .as_deref()
                        .map(|f| self.abbreviate(f))
                        .unwrap_or_default()
                );
                let label = format!("{}  {}", i + 1, work.name);
                canvas.push(choice_row(
                    &label,
                    &detail,
                    self.selected == HOME_OPTIONS + i,
                    true,
                    18,
                ));
            }
        }
        canvas.blank();
        let close = if self.options.can_close {
            " · esc back"
        } else {
            ""
        };
        canvas.hint(&format!(
            "↑↓ move · ⏎ open · 1-9 recent · q quick · t templates{close}"
        ));
    }

    fn draw_templates(&self, canvas: &mut Canvas) {
        canvas.logo();
        canvas.push(Line::from(Span::styled("choose a template", dim())));
        canvas.blank();
        // One line each, in columns sized to the longest entry:
        // `1  feat      Feature    add something new    (lead + tester)`.
        let templates = &self.options.templates;
        let kind_of = |t: &Template| t.kind.map(WorkKind::id).unwrap_or_default();
        let kind_width = templates
            .iter()
            .map(|t| kind_of(t).len())
            .max()
            .unwrap_or(0)
            + 2;
        let label_width = templates
            .iter()
            .map(|t| t.label.chars().count())
            .max()
            .unwrap_or(0);
        let description_width = templates
            .iter()
            .map(|t| t.description.chars().count())
            .max()
            .unwrap_or(0)
            + 3;
        for (i, template) in templates.iter().enumerate() {
            let label = format!(
                "{}  {:<kind_width$}{}",
                i + 1,
                kind_of(template),
                template.label
            );
            let access = if template.read_only {
                " · read-only"
            } else {
                ""
            };
            let detail = format!(
                "{:<description_width$}({}{access})",
                template.description,
                template.team_summary()
            );
            canvas.push(choice_row(
                &label,
                &detail,
                self.selected == i,
                true,
                3 + kind_width + label_width + 3,
            ));
        }
        canvas.blank();
        canvas.hint("↑↓ move · ⏎ choose · 1-9 pick · esc back");
    }

    fn draw_form(&self, form: &WorkForm, canvas: &mut Canvas) {
        let template = self.template(form);
        let (kind, about) = template.map_or_else(
            || ("quick start".to_owned(), "one agent in a folder".to_owned()),
            |t| {
                (
                    t.kind
                        .map_or_else(|| t.label.clone(), |k| k.id().to_owned()),
                    t.description.clone(),
                )
            },
        );
        let badge = form.form.mode_label().map_or_else(Line::default, |mode| {
            let style = if mode == "INS" {
                Style::new().fg(Color::Black).bg(Color::Green)
            } else {
                Style::new().fg(Color::Black).bg(ACCENT)
            };
            Line::from(vec![
                Span::styled(format!(" {mode} "), style.add_modifier(Modifier::BOLD)),
                Span::raw(" "),
            ])
        });
        canvas.card = Some((
            Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    "new work",
                    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" · {kind} "),
                    Style::new().add_modifier(Modifier::BOLD),
                ),
            ]),
            badge,
        ));
        canvas.push(Line::from(Span::styled(about, dim())));
        canvas.blank();
        // A blank row between fields when the window has room for it.
        let compact = form
            .form
            .lines(canvas.width, &Theme::default(), false)
            .0
            .len();
        let spacious = compact * 3 / 2 + 12 <= usize::from(canvas.body.height);
        let (lines, cursor) = form.form.lines(canvas.width, &Theme::default(), spacious);
        let base = canvas.lines.len();
        for line in lines {
            canvas.push(line);
        }
        canvas.cursor = cursor.map(|(line, column)| (base + line, column));
        canvas.blank();
        let mut hints: Vec<String> = form
            .form
            .hints()
            .into_iter()
            .map(|(keys, what)| format!("{keys} {what}"))
            .collect();
        if !form.form.is_inserting() {
            hints.push("esc back".into());
        }
        canvas.hint(&hints.join(" · "));
    }

    fn draw_picker(&self, form: &WorkForm, query: &LineEditor, canvas: &mut Canvas) {
        let name = match form.form.value(NAME) {
            Value::Text(name) => name,
            _ => String::new(),
        };
        canvas.card = Some((
            Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    "choose a folder",
                    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" · for {name} "), dim()),
            ]),
            Line::default(),
        ));
        canvas.prompt(query, "type to filter, or a path starting with / or ~");
        canvas.blank();
        let candidates = self.picker_candidates(&query.text());
        if candidates.is_empty() {
            canvas.push(Line::from(Span::styled("no folders match", dim())));
        }
        let visible = 12;
        let start = self.selected.saturating_sub(visible - 1);
        for (i, folder) in candidates.iter().enumerate().skip(start).take(visible) {
            let selected = i == self.selected;
            let marker = if selected { "▸ " } else { "  " };
            let style = if selected {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            };
            canvas.push(Line::from(vec![
                Span::styled(marker, Style::new().fg(ACCENT)),
                Span::styled(self.abbreviate(folder), style),
            ]));
        }
        canvas.blank();
        canvas.hint("type to filter · ↑↓ move · ⏎ choose · esc back");
    }

    /// The inverse of [`Self::abbreviate`]: `~/x` back to a full path.
    fn expand(&self, shown: &str) -> String {
        match shown.strip_prefix('~') {
            Some(rest)
                if !self.options.home.is_empty() && (rest.is_empty() || rest.starts_with('/')) =>
            {
                format!("{}{rest}", self.options.home)
            }
            _ => shown.to_owned(),
        }
    }

    fn abbreviate(&self, path: &str) -> String {
        let path = normalize_path(path);
        match path.strip_prefix(&self.options.home) {
            Some(rest)
                if !self.options.home.is_empty() && (rest.is_empty() || rest.starts_with('/')) =>
            {
                format!("~{rest}")
            }
            _ => path,
        }
    }
}

/// Field keys in the new-work form.
const FOLDER: &str = "folder";
const NAME: &str = "name";
const LEAD: &str = "lead";
const CHILDREN: &str = "children";
const CHILD: &str = "child";
const PERMITS: &str = "permits";
const START: &str = "start";

/// The first brief question, which must be answered.
const FIRST_QUESTION: &str = "q0";

/// The key of brief question `index`.
fn question_key(index: usize) -> String {
    format!("q{index}")
}

/// The question index in a brief field's key.
fn question_index(key: &str) -> Option<usize> {
    key.strip_prefix('q')?.parse().ok()
}

/// `path` without a trailing slash (`/Users/me/` is `/Users/me`), except
/// the root itself.
fn normalize_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() && path.starts_with('/') {
        "/".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// The last path component, as a default work name.
fn folder_name(folder: &str) -> String {
    folder
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(folder)
        .to_owned()
}

fn dim() -> Style {
    Style::new().fg(MUTED)
}

/// `▸ label   detail`, highlighted when selected, dimmed when disabled.
fn choice_row(
    label: &str,
    detail: &str,
    selected: bool,
    enabled: bool,
    label_width: usize,
) -> Line<'static> {
    let marker = Span::styled(if selected { "▸ " } else { "  " }, Style::new().fg(ACCENT));
    let label_style = match (selected, enabled) {
        (_, false) => dim(),
        (true, true) => Style::new().add_modifier(Modifier::BOLD),
        (false, true) => Style::new(),
    };
    Line::from(vec![
        marker,
        Span::styled(format!("{label:<label_width$}"), label_style),
        Span::styled(detail.to_owned(), dim()),
    ])
}

/// A column of lines, centred horizontally and placed a little above the
/// middle (where a centred block looks centred).
struct Canvas {
    /// The widest the column may be.
    width: u16,
    /// Centre on the widest line rather than on `width`.
    fit: bool,
    /// Draw the column in a rounded box with this title (left) and badge
    /// (right): the form and folder picker are cards.
    card: Option<(Line<'static>, Line<'static>)>,
    body: Rect,
    lines: Vec<Line<'static>>,
    /// (line index, column) of the cursor.
    cursor: Option<(usize, u16)>,
}

impl Canvas {
    fn push(&mut self, line: Line<'static>) {
        self.lines.push(line);
    }

    fn blank(&mut self) {
        self.lines.push(Line::raw(""));
    }

    fn hint(&mut self, text: &str) {
        self.push(Line::from(Span::styled(text.to_owned(), dim())));
    }

    /// The block logo when there's room for it, else the word.
    fn logo(&mut self) {
        let big = self.width >= 49 && self.body.height >= 28;
        for line in draw_logo(big) {
            self.push(line);
        }
        self.blank();
    }

    /// A `› ` prompt with a line of text; the cursor sits after the
    /// prompt, on a space before the dim hint while it's empty.
    fn prompt(&mut self, editor: &LineEditor, hint: &str) {
        let prefix = 2;
        let mut spans = vec![Span::styled(
            "› ",
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        )];
        let text = editor.text();
        let room = usize::from(self.width).saturating_sub(prefix + 1);
        let skip = editor.cursor().saturating_sub(room);
        if text.is_empty() {
            spans.push(Span::styled(format!(" {hint}"), dim()));
        } else {
            spans.push(Span::raw(
                text.chars().skip(skip).take(room).collect::<String>(),
            ));
        }
        let column = u16::try_from(prefix + editor.cursor() - skip).unwrap_or(0);
        self.cursor = Some((self.lines.len(), column));
        self.push(Line::from(spans));
    }

    /// Draws the column; returns the cursor.
    fn render(self, buffer: &mut Buffer) -> Option<(u16, u16)> {
        let width = if self.fit {
            let widest = self.lines.iter().map(Line::width).max().unwrap_or(0);
            u16::try_from(widest).unwrap_or(u16::MAX).min(self.width)
        } else {
            self.width
        };
        let height = u16::try_from(self.lines.len()).unwrap_or(u16::MAX);
        // A card adds its border and padding (1 row, 3 columns a side).
        let (pad_x, pad_y) = if self.card.is_some() { (4, 2) } else { (0, 0) };
        let outer = (width + 2 * pad_x, height + 2 * pad_y);
        let left = self.body.x + self.body.width.saturating_sub(outer.0) / 2;
        // 2/5 of the spare rows above, 3/5 below.
        let above = self.body.y + self.body.height.saturating_sub(outer.1) * 2 / 5;
        if let Some((title, badge)) = self.card {
            let area = Rect::new(left, above, outer.0, outer.1).intersection(self.body);
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(dim())
                .title_top(title)
                .title_top(badge.right_aligned())
                .render(area, buffer);
        }
        let x = left + pad_x;
        let top = above + pad_y;
        // Lines that don't fit are dropped from the bottom.
        let mut cursor = None;
        for (i, line) in self.lines.into_iter().enumerate() {
            let y = top + u16::try_from(i).unwrap_or(u16::MAX);
            if y >= self.body.bottom().saturating_sub(pad_y) {
                break;
            }
            Paragraph::new(line).render(Rect::new(x, y, width, 1), buffer);
            if self.cursor.is_some_and(|(line, _)| line == i) {
                cursor = self.cursor.map(|(_, column)| (x + column, y));
            }
        }
        cursor
    }
}

#[cfg(test)]
mod tests {
    use makima_core::{BriefQuestion, TeamMember, builtin_presets};

    use super::*;

    /// A fixed id for the one recent work.
    fn recent_id() -> WorkspaceId {
        "0199a000-0000-7000-8000-000000000001"
            .parse()
            .unwrap_or_else(|e| panic!("{e}"))
    }

    fn options() -> FlowOptions {
        let mut fix = Template::new("fix", "Fix", "claude");
        fix.kind = Some(WorkKind::Fix);
        fix.description = "something is broken".into();
        fix.children = vec![TeamMember::new(None, AgentRole::Tester)];
        fix.questions = vec![
            BriefQuestion::new("What's broken?", "the symptom"),
            BriefQuestion::new("How to reproduce", "steps"),
        ];
        FlowOptions::new(builtin_presets(), vec![fix])
            .with_home("/u")
            .with_folders(vec!["/u/code/makima".into(), "/u/code/app".into()])
            .with_recents(vec![RecentWork::new(
                recent_id(),
                "login",
                Some(WorkKind::Fix),
                Some("/u/code/app".into()),
            )])
    }

    fn keys(flow: &mut NewWorkFlow, input: &[&[u8]]) -> Vec<FlowEvent> {
        input.iter().flat_map(|bytes| flow.feed(bytes)).collect()
    }

    fn started(events: Vec<FlowEvent>) -> NewWorkRequest {
        match events.into_iter().last() {
            Some(FlowEvent::Start(request)) => request,
            other => panic!("expected Start, got {other:?}"),
        }
    }

    #[test]
    fn enter_twice_starts_a_quick_session() {
        let mut flow = NewWorkFlow::new(options());
        let events = keys(&mut flow, &[b"\r", b"\r"]);
        let request = started(events);
        assert_eq!(request.name, "makima");
        assert_eq!(request.folder, "/u/code/makima");
        assert_eq!(request.lead, "claude");
        assert!(request.children.is_empty() && request.template.is_none());
    }

    #[test]
    fn a_template_fills_the_team_and_the_brief() {
        let mut flow = NewWorkFlow::new(options());
        // Templates, first template; the form starts at the top, so down
        // to the first question (past folder, name, lead, children, they
        // run, permits).
        keys(&mut flow, &[b"t", b"\r"]);
        keys(&mut flow, &[b"\t" as &[u8]; 6]);
        let events = keys(&mut flow, &[b"login loops", b"\r", b"open /login", b"\r"]);
        assert!(events.is_empty());
        let request = started(keys(&mut flow, &[b"\r"]));
        assert_eq!(request.kind, Some(WorkKind::Fix));
        assert_eq!(request.lead, "claude");
        assert_eq!(
            request.children,
            vec![ChildRequest {
                preset: "claude".into(),
                role: AgentRole::Tester
            }]
        );
        assert_eq!(
            request.answers,
            vec![
                ("What's broken?".into(), "login loops".into()),
                ("How to reproduce".into(), "open /login".into())
            ]
        );
    }

    #[test]
    fn changing_the_team() {
        let mut flow = NewWorkFlow::new(options());
        // From the top: down to "lead" (codex), "children" (3), "they
        // run" (codex: same as lead, shell, claude, codex), then the brief.
        let (down, right) = (b"\t" as &[u8], b"\x1b[C" as &[u8]);
        keys(&mut flow, &[b"t", b"\r", down, down, right]);
        keys(&mut flow, &[down, b"3", down, right, right, right]);
        keys(&mut flow, &[down, down, b"login loops"]);
        keys(&mut flow, &[down as &[u8]; 4]);
        let request = started(keys(&mut flow, &[b"\r"]));
        assert_eq!(request.lead, "codex");
        assert_eq!(request.children.len(), 3);
        assert!(
            request.children.iter().all(|c| c.preset == "codex"),
            "{:?}",
            request.children
        );
        assert_eq!(request.children[0].role, AgentRole::Tester);
        assert_eq!(request.children[2].role, AgentRole::Implementer);
    }

    #[test]
    fn children_have_no_cap() {
        let mut flow = NewWorkFlow::new(options());
        // Quick start: up past permits to children, type 12, down to start.
        keys(&mut flow, &[b"\r", b"\x1b[A", b"\x1b[A", b"1", b"2"]);
        assert!(
            screen_text(&flow).contains("they run"),
            "shown once there are children"
        );
        keys(&mut flow, &[b"\t" as &[u8]; 4]);
        let request = started(keys(&mut flow, &[b"\r"]));
        assert_eq!(request.children.len(), 12);
    }

    #[test]
    fn permissions_default_from_the_template_and_can_change() {
        let mut flow = NewWorkFlow::new(options());
        // Quick start: "default" unless changed.
        assert_eq!(started(keys(&mut flow, &[b"\r", b"\r"])).permission, None);

        let mut flow = NewWorkFlow::new(options());
        // Quick start, up to permits, two to the right: manual, accept-edits.
        keys(&mut flow, &[b"\r", b"\x1b[A", b"\x1b[C", b"\x1b[C", b"\t"]);
        let request = started(keys(&mut flow, &[b"\r"]));
        assert_eq!(request.permission, Some(PermissionMode::AcceptEdits));
    }

    #[test]
    fn folder_picker_filters_and_renames() {
        let mut flow = NewWorkFlow::new(options());
        // Quick start, up to the folder (start → permits → children →
        // agent → name → folder), pick "app".
        let up = b"\x1b[A" as &[u8];
        keys(
            &mut flow,
            &[b"\r", up, up, up, up, up, b"\r", b"app", b"\r"],
        );
        keys(&mut flow, &[b"\t" as &[u8]; 5]);
        let request = started(keys(&mut flow, &[b"\r"]));
        assert_eq!(request.folder, "/u/code/app");
        assert_eq!(
            request.name, "app",
            "the name follows the folder until edited"
        );
    }

    #[test]
    fn typing_on_quick_start_names_the_work() {
        let mut flow = NewWorkFlow::new(options());
        let request = started(keys(&mut flow, &[b"\r", b"a", b"u", b"t", b"h", b"\r"]));
        assert_eq!(request.name, "auth");
        assert_eq!(request.folder, "/u/code/makima", "the folder stays");
    }

    #[test]
    fn mistakes_show_under_their_field() {
        let mut flow = NewWorkFlow::new(options());
        // Quick start, up to the name, clear it, start.
        let up = b"\x1b[A" as &[u8];
        keys(&mut flow, &[b"\r", up, up, up, up, b"\x15"]);
        keys(&mut flow, &[b"\t" as &[u8]; 4]);
        assert!(keys(&mut flow, &[b"\r"]).is_empty());
        assert!(screen_text(&flow).contains("✗ give the work a name"));
    }

    #[test]
    fn recents_reopen_and_build_your_own_is_disabled() {
        let mut flow = NewWorkFlow::new(options());
        assert_eq!(flow.feed(b"1"), vec![FlowEvent::Reopen(recent_id())]);
        keys(&mut flow, &[b"\x1b[B", b"\x1b[B", b"\r"]);
        assert_eq!(
            flow.notice.as_deref(),
            Some("building your own templates is coming soon")
        );
        assert!(flow.feed(b"\x1b").is_empty(), "nothing to close back to");
    }

    #[test]
    fn voice_goes_into_the_focused_question() {
        let mut flow = NewWorkFlow::new(options());
        keys(&mut flow, &[b"t", b"\r"]);
        keys(&mut flow, &[b"\t" as &[u8]; 6]);
        flow.add_voice(
            VoiceClip::new("/tmp/a.m4a", "only on Safari", "apple")
                .with_duration_ms(Some(900))
                .with_language(Some("en-US".into())),
        );
        keys(&mut flow, &[b"\r", b"\r"]);
        let request = started(keys(&mut flow, &[b"\r"]));
        assert_eq!(request.answers[0].1, "only on Safari");
        assert_eq!(request.voice[0].question.as_deref(), Some("What's broken?"));
    }

    /// Every screen of the flow, as a user steps through it.
    fn every_screen() -> Vec<(&'static str, NewWorkFlow)> {
        let options = || {
            let mut fix = Template::new("fix", "Fix", "claude");
            fix.kind = Some(WorkKind::Fix);
            fix.description = "something is broken".into();
            fix.questions = vec![BriefQuestion::new("What's broken?", "the symptom")];
            // Real homes come with a trailing slash.
            FlowOptions::new(builtin_presets(), vec![fix])
                .with_folders(vec![
                    "/Users/me/code/app".into(),
                    "/Users/me/code/api".into(),
                ])
                .with_home("/Users/me/")
                .with_recents(vec![RecentWork::new(
                    recent_id(),
                    "login",
                    Some(WorkKind::Fix),
                    Some("/Users/me/code/app".into()),
                )])
        };
        let at = |keys: &[&[u8]]| {
            let mut flow = NewWorkFlow::new(options());
            for k in keys {
                flow.feed(k);
            }
            flow
        };
        let up = b"\x1b[A" as &[u8];
        vec![
            ("home", at(&[])),
            ("templates", at(&[b"t"])),
            ("quick form", at(&[b"\r"])),
            ("template form", at(&[b"t", b"\r"])),
            ("helix form", {
                let mut flow = NewWorkFlow::new(options().modal(true));
                flow.feed(b"\r");
                flow
            }),
            ("helix form, editing", {
                let mut flow = NewWorkFlow::new(options().modal(true));
                for k in [b"\r" as &[u8], b"i"] {
                    flow.feed(k);
                }
                flow
            }),
            ("picker", at(&[b"\r", up, up, up, up, up, b"\r"])),
            (
                "picker, filtered",
                at(&[b"\r", up, up, up, up, up, b"\r", b"ap"]),
            ),
        ]
    }

    /// The layout rules every screen keeps, at several window sizes: paths
    /// are shown from `~`, the cursor never sits on a hint's text, menus
    /// are centred, and nothing runs past the edge.
    #[test]
    fn layout_rules_hold_on_every_screen() {
        for (columns, rows) in [(80, 30), (120, 40), (200, 60)] {
            for (name, flow) in every_screen() {
                let area = Rect::new(0, 0, columns, rows);
                let mut buffer = Buffer::empty(area);
                let cursor = flow.draw(area, &mut buffer);
                let lines: Vec<String> = (0..rows)
                    .map(|y| (0..columns).map(|x| buffer[(x, y)].symbol()).collect())
                    .collect();
                let at = format!("{name} at {columns}×{rows}");
                if name.starts_with("picker") {
                    assert!(
                        lines.iter().any(|l| l.contains("choose a folder")),
                        "{at}: not the picker"
                    );
                    assert!(
                        lines.iter().any(|l| l.contains("for app")),
                        "{at}: the work's name"
                    );
                }
                for line in &lines {
                    assert!(
                        !line.contains("/Users/me"),
                        "{at}: unabbreviated path in {line:?}"
                    );
                    assert!(
                        line.ends_with(' ') || line.trim().is_empty(),
                        "{at}: text runs to the edge in {line:?}"
                    );
                }
                if let Some((x, y)) = cursor {
                    let under = buffer[(x, y)].symbol();
                    let typed = buffer[(x.saturating_sub(1), y)].symbol();
                    assert!(
                        under == " " || typed != " ",
                        "{at}: the cursor sits on {under:?}, inside a hint"
                    );
                }
                // Menus and form cards alike sit in the middle.
                {
                    let used: Vec<&String> =
                        lines.iter().filter(|l| !l.trim().is_empty()).collect();
                    let left = used
                        .iter()
                        .map(|l| l.len() - l.trim_start().len())
                        .min()
                        .unwrap_or(0);
                    let right = used
                        .iter()
                        .map(|l| l.chars().count() - l.trim_end().chars().count())
                        .min()
                        .unwrap_or(0);
                    assert!(
                        left.abs_diff(right) <= 1,
                        "{at}: off centre ({left} left, {right} right)"
                    );
                }
            }
        }
    }

    #[test]
    fn helix_forms_move_with_j_k_and_edit_with_i() {
        let mut flow = NewWorkFlow::new(options().modal(true));
        // Quick start: on start, in normal mode. Letters are commands.
        keys(&mut flow, &[b"\r", b"x"]);
        assert!(screen_text(&flow).contains("NOR"));
        // i on start edits the name; type, Esc, then ⏎ on start.
        keys(&mut flow, &[b"i", b"\x15", b"auth"]);
        assert!(screen_text(&flow).contains("INS"));
        keys(&mut flow, &[b"\x1b"]);
        assert!(
            screen_text(&flow).contains("NOR"),
            "Esc stops editing, not the form"
        );
        // From the name: k to the folder, then j ×4 down to permits, l.
        keys(&mut flow, &[b"k", b"j", b"j", b"j", b"j", b"l"]);
        keys(&mut flow, &[b"G"]);
        let request = started(keys(&mut flow, &[b"\r"]));
        assert_eq!(request.name, "auth");
        assert_eq!(request.permission, Some(PermissionMode::Manual));

        // c replaces; ⏎ after `i` from start starts straight away.
        let mut flow = NewWorkFlow::new(options().modal(true));
        let request = started(keys(&mut flow, &[b"\r", b"c", b"api", b"\r"]));
        assert_eq!(request.name, "api");

        // Esc in normal mode leaves the form.
        let mut flow = NewWorkFlow::new(options().modal(true));
        keys(&mut flow, &[b"\r", b"\x1b"]);
        assert!(screen_text(&flow).contains("Quick start"));
    }

    #[test]
    fn template_work_needs_the_first_answer() {
        let mut flow = NewWorkFlow::new(options());
        // Template form, straight to start without describing anything.
        keys(&mut flow, &[b"t", b"\r"]);
        keys(&mut flow, &[b"\t" as &[u8]; 9]);
        assert!(
            keys(&mut flow, &[b"\r"]).is_empty(),
            "no start with an empty brief"
        );
        assert!(screen_text(&flow).contains("answer this first"));
    }

    #[test]
    fn keys_on_the_folder_never_rename_the_work() {
        let mut flow = NewWorkFlow::new(options());
        let up = b"\x1b[A" as &[u8];
        // Quick start, up to the folder, then a stray Space and a letter.
        keys(&mut flow, &[b"\r", up, up, up, up, up, b" ", b"x"]);
        keys(&mut flow, &[b"\t" as &[u8]; 5]);
        let request = started(keys(&mut flow, &[b"\r"]));
        assert_eq!(request.name, "makima");
    }

    fn screen_text(flow: &NewWorkFlow) -> String {
        let area = Rect::new(0, 0, 80, 40);
        let mut buffer = Buffer::empty(area);
        flow.draw(area, &mut buffer);
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    .trim_end()
                    .to_owned()
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim_matches('\n')
            .to_owned()
    }

    #[test]
    fn screens() {
        let mut flow = NewWorkFlow::new(options());
        insta::assert_snapshot!("home", screen_text(&flow));
        keys(&mut flow, &[b"t"]);
        insta::assert_snapshot!("templates", screen_text(&flow));
        keys(&mut flow, &[b"\r"]);
        keys(&mut flow, &[b"\t" as &[u8]; 6]);
        keys(
            &mut flow,
            &[b"login loops on Safari after the session expires"],
        );
        insta::assert_snapshot!("form", screen_text(&flow));
    }
}
