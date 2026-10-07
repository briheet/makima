//! The terminal chrome for Swift: rendering, the talk-line editor, `@`
//! routing, `makima>` commands, the Helix/tmux key maps and the picker, all from
//! `makima-tui`. Pane ids cross as [`AgentId`]s.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use makima_core::{AgentId, PresetId};
use makima_tui as tui;

use crate::{NewWorkFlow, UnknownToBridge};

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChromeTab {
    pub label: String,
    pub active: bool,
    pub attention: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ChromePaneRole {
    Lead,
    Child { number: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ChromePaneStatus {
    Starting,
    Working,
    NeedsInput,
    Done,
    Failed,
    Exited,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChromePane {
    pub id: AgentId,
    pub name: String,
    pub role: ChromePaneRole,
    pub status: ChromePaneStatus,
    pub detail: String,
    pub focused: bool,
    /// Started with permission checks off; flagged in red.
    pub unguarded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ChromeMode {
    /// Keys go to the focused pane (Helix's INS).
    Pane,
    /// Keys are Makima commands (Helix's NOR; tmux's prefix).
    Keys,
    /// Picking from a fuzzy list.
    Picker,
    /// Keys go to a tool's floating window.
    Tool,
    Talk,
    Command,
    Confirm,
    NewWork,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChromeTalkLine {
    pub prompt: String,
    pub text: String,
    pub cursor: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, uniffi::Record)]
pub struct ChromeCounts {
    pub working: u16,
    pub waiting: u16,
    pub done: u16,
    pub failed: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChromeState {
    pub tabs: Vec<ChromeTab>,
    pub panes: Vec<ChromePane>,
    pub zoomed: Option<AgentId>,
    pub mode: ChromeMode,
    pub talk: ChromeTalkLine,
    pub counts: ChromeCounts,
    pub work_name: Option<String>,
    pub clock: String,
    pub show_help: bool,
    pub confirm: Option<String>,
    pub notice: Option<String>,
    /// A tool open over the panes.
    pub float: Option<ChromeFloat>,
    /// How the key into Makima's keys is written in hints, e.g. `^Space`.
    pub leader: String,
}

/// A tool's floating window.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChromeFloat {
    /// The tool's label, e.g. `lazygit`.
    pub title: String,
    /// Where it runs.
    pub folder: String,
    /// Share of the body, 30 to 100.
    pub percent: u8,
    pub zoomed: bool,
    /// How to hide it, e.g. `^Space space g hide`.
    pub hint: String,
}

/// A rectangle in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ChromeCellRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChromePaneRect {
    pub id: AgentId,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ChromeTabHit {
    pub index: u32,
    pub start: u16,
    pub end: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChromeFrame {
    /// Draws the whole chrome; feed it to the chrome terminal.
    pub ansi: String,
    pub panes: Vec<ChromePaneRect>,
    pub tabs: Vec<ChromeTabHit>,
    /// Where the open tool's terminal goes.
    pub float: Option<ChromeCellRect>,
    /// What must show above the agents' terminals (key menu, help, picker,
    /// a tool's border): draw `ansi` on a terminal the size of `rect`.
    pub overlay: Option<ChromeOverlay>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChromeOverlay {
    pub rect: ChromeCellRect,
    pub ansi: String,
}

/// Renders the chrome for a `columns` × `rows` terminal. `keys` sets the
/// mode badge and hints and shows a pending minor mode's keys; with a
/// `flow`, the new-work flow fills the body, and a `picker` is drawn over it.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI passes records by value"
)]
#[must_use]
pub fn render_chrome(
    state: ChromeState,
    keys: Arc<KeymapEngine>,
    flow: Option<Arc<NewWorkFlow>>,
    picker: Option<Arc<Picker>>,
    columns: u16,
    rows: u16,
) -> ChromeFrame {
    let flow = flow.as_ref().map(|f| f.flow());
    let picker = picker.as_ref().map(|p| p.picker());
    let keymap = keys.keymap();
    let mut state = state.to_tui();
    state.key_style = keymap.style();
    state.menu = keymap.menu();
    if state.show_help {
        state.bindings = keymap.help();
    }
    drop(keymap);
    let frame = tui::render(&state, flow.as_deref(), picker.as_deref(), columns, rows);
    ChromeFrame {
        ansi: frame.ansi,
        panes: frame
            .panes
            .into_iter()
            .map(|p| ChromePaneRect {
                id: p.id,
                x: p.x,
                y: p.y,
                width: p.width,
                height: p.height,
            })
            .collect(),
        tabs: frame
            .tabs
            .into_iter()
            .map(|t| ChromeTabHit {
                index: u32::try_from(t.index).unwrap_or(u32::MAX),
                start: t.start,
                end: t.end,
            })
            .collect(),
        float: frame.float.map(|r| ChromeCellRect {
            x: r.x,
            y: r.y,
            width: r.width,
            height: r.height,
        }),
        overlay: frame.overlay.map(|o| ChromeOverlay {
            rect: ChromeCellRect {
                x: o.rect.x,
                y: o.rect.y,
                width: o.rect.width,
                height: o.rect.height,
            },
            ansi: o.ansi,
        }),
    }
}

impl ChromeState {
    fn to_tui(&self) -> tui::ChromeState {
        tui::ChromeState {
            tabs: self
                .tabs
                .iter()
                .map(|t| tui::TabSpec {
                    label: t.label.clone(),
                    active: t.active,
                    attention: t.attention,
                })
                .collect(),
            panes: self
                .panes
                .iter()
                .map(|p| tui::PaneSpec {
                    id: p.id,
                    name: p.name.clone(),
                    role: match p.role {
                        ChromePaneRole::Lead => tui::PaneRole::Lead,
                        ChromePaneRole::Child { number } => tui::PaneRole::Child(number),
                    },
                    status: match p.status {
                        ChromePaneStatus::Starting => tui::PaneStatus::Starting,
                        ChromePaneStatus::Working => tui::PaneStatus::Working,
                        ChromePaneStatus::NeedsInput => tui::PaneStatus::NeedsInput,
                        ChromePaneStatus::Done => tui::PaneStatus::Done,
                        ChromePaneStatus::Failed => tui::PaneStatus::Failed,
                        ChromePaneStatus::Exited => tui::PaneStatus::Exited,
                    },
                    detail: p.detail.clone(),
                    focused: p.focused,
                    unguarded: p.unguarded,
                })
                .collect(),
            zoomed: self.zoomed,
            mode: match self.mode {
                ChromeMode::Pane => tui::ChromeMode::Pane,
                ChromeMode::Keys => tui::ChromeMode::Keys,
                ChromeMode::Picker => tui::ChromeMode::Picker,
                ChromeMode::Tool => tui::ChromeMode::Tool,
                ChromeMode::Talk => tui::ChromeMode::Talk,
                ChromeMode::Command => tui::ChromeMode::Command,
                ChromeMode::Confirm => tui::ChromeMode::Confirm,
                ChromeMode::NewWork => tui::ChromeMode::NewWork,
            },
            talk: tui::TalkLine {
                prompt: self.talk.prompt.clone(),
                text: self.talk.text.clone(),
                cursor: usize::try_from(self.talk.cursor).unwrap_or(usize::MAX),
            },
            counts: tui::StatusCounts {
                working: self.counts.working,
                waiting: self.counts.waiting,
                done: self.counts.done,
                failed: self.counts.failed,
            },
            work_name: self.work_name.clone(),
            clock: self.clock.clone(),
            show_help: self.show_help,
            confirm: self.confirm.clone(),
            notice: self.notice.clone(),
            key_style: tui::KeyStyle::default(),
            menu: None,
            bindings: Vec::new(),
            leader: self.leader.clone(),
            float: self.float.as_ref().map(|f| {
                tui::FloatSpec::new(f.title.clone(), f.folder.clone(), f.percent)
                    .zoomed(f.zoomed)
                    .with_hint(f.hint.clone())
            }),
        }
    }
}

// MARK: - Talk line editor

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum TalkEditorEvent {
    Submitted { line: String },
    Cancelled,
}

/// The talk line's editor, fed the raw bytes the chrome terminal produces
/// for each key.
#[derive(Debug, Default, uniffi::Object)]
pub struct TalkEditor(Mutex<tui::LineEditor>);

#[uniffi::export]
impl TalkEditor {
    #[uniffi::constructor]
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Applies one chunk of terminal input.
    #[must_use]
    pub fn feed(&self, input: &[u8]) -> Vec<TalkEditorEvent> {
        self.editor()
            .feed(input)
            .into_iter()
            .map(|event| match event {
                tui::EditorEvent::Submitted(line) => TalkEditorEvent::Submitted { line },
                // Cancelled, and any event the editor gains later.
                _ => TalkEditorEvent::Cancelled,
            })
            .collect()
    }

    #[must_use]
    pub fn text(&self) -> String {
        self.editor().text()
    }

    /// The cursor, in characters.
    #[must_use]
    pub fn cursor(&self) -> u32 {
        u32::try_from(self.editor().cursor()).unwrap_or(u32::MAX)
    }

    pub fn set_text(&self, text: &str) {
        self.editor().set_text(text);
    }
}

impl TalkEditor {
    /// A panic mid-edit leaves at worst a half-edited line, so poisoning is
    /// ignored.
    fn editor(&self) -> MutexGuard<'_, tui::LineEditor> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

// MARK: - Routing and commands

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TalkTarget {
    Lead,
    All,
    Children,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TalkRecipient {
    pub id: AgentId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RoutedTalk {
    pub recipients: Vec<AgentId>,
    pub text: String,
}

/// A mistake in something typed at the talk line or `makima>` prompt.
/// Its message (with a usage hint) comes from [`input_error_message`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Error)]
#[non_exhaustive]
pub enum InputError {
    /// The talk line had no message.
    EmptyMessage,
    /// `@name` matches no agent.
    UnknownAgent {
        name: String,
    },
    /// The work has no agents to talk to.
    NoAgents,
    UnknownCommand {
        command: String,
    },
    /// `new` without a name.
    MissingName,
    MissingValue {
        flag: String,
    },
    BadCount {
        value: String,
    },
    TooManyArguments,
    UnknownOption {
        option: String,
    },
    UnclosedQuote,
    /// A mistake this version of the bridge has no case for yet.
    Other {
        message: String,
    },
}

impl From<tui::TalkError> for InputError {
    fn from(error: tui::TalkError) -> Self {
        match error {
            tui::TalkError::Empty => Self::EmptyMessage,
            tui::TalkError::UnknownAgent(name) => Self::UnknownAgent { name },
            tui::TalkError::NoAgents => Self::NoAgents,
            other => Self::Other {
                message: other.to_string(),
            },
        }
    }
}

impl From<tui::CommandError> for InputError {
    fn from(error: tui::CommandError) -> Self {
        match error {
            tui::CommandError::Unknown(command) => Self::UnknownCommand { command },
            tui::CommandError::MissingName => Self::MissingName,
            tui::CommandError::MissingValue { flag } => Self::MissingValue { flag },
            tui::CommandError::BadCount(value) => Self::BadCount { value },
            tui::CommandError::TooManyArguments => Self::TooManyArguments,
            tui::CommandError::UnknownOption(option) => Self::UnknownOption { option },
            tui::CommandError::UnclosedQuote => Self::UnclosedQuote,
            other => Self::Other {
                message: other.to_string(),
            },
        }
    }
}

/// The messages are `makima-tui`'s, so they're written in one place.
impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self.clone() {
            Self::EmptyMessage => tui::TalkError::Empty.to_string(),
            Self::UnknownAgent { name } => tui::TalkError::UnknownAgent(name).to_string(),
            Self::NoAgents => tui::TalkError::NoAgents.to_string(),
            Self::UnknownCommand { command } => tui::CommandError::Unknown(command).to_string(),
            Self::MissingName => tui::CommandError::MissingName.to_string(),
            Self::MissingValue { flag } => tui::CommandError::MissingValue { flag }.to_string(),
            Self::BadCount { value } => tui::CommandError::BadCount(value).to_string(),
            Self::TooManyArguments => tui::CommandError::TooManyArguments.to_string(),
            Self::UnknownOption { option } => tui::CommandError::UnknownOption(option).to_string(),
            Self::UnclosedQuote => tui::CommandError::UnclosedQuote.to_string(),
            Self::Other { message } => message,
        };
        f.write_str(&message)
    }
}

impl std::error::Error for InputError {}

/// What to show for `error`, with a usage hint where one helps.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI passes enums by value"
)]
#[must_use]
pub fn input_error_message(error: InputError) -> String {
    error.to_string()
}

/// Works out who `input` (optionally starting with `@target`) is for.
///
/// # Errors
/// [`InputError::EmptyMessage`], [`InputError::UnknownAgent`] or
/// [`InputError::NoAgents`].
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI passes records by value"
)]
pub fn route_talk(
    input: &str,
    default_target: TalkTarget,
    lead: Option<TalkRecipient>,
    children: Vec<TalkRecipient>,
) -> Result<RoutedTalk, InputError> {
    let recipient = |r: &TalkRecipient| tui::Recipient {
        id: r.id,
        name: r.name.clone(),
    };
    let default = match default_target {
        TalkTarget::Lead => tui::Target::Lead,
        TalkTarget::All => tui::Target::All,
        TalkTarget::Children => tui::Target::Children,
    };
    let lead = lead.as_ref().map(recipient);
    let children: Vec<_> = children.iter().map(recipient).collect();
    let routed = tui::route_talk(input, default, lead.as_ref(), &children)?;
    Ok(RoutedTalk {
        recipients: routed.recipients,
        text: routed.text,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MakimaCommand {
    New {
        name: String,
        /// `None` runs the configured lead (`Core::default_lead`).
        lead: Option<PresetId>,
        children: u32,
        child: Option<PresetId>,
    },
    /// More children for the work on screen; `preset` is the lead's when
    /// `None`.
    Add {
        preset: Option<PresetId>,
        count: u32,
    },
    Help,
}

/// Parses a `makima>` line.
///
/// # Errors
/// [`InputError`] for a command it can't parse.
#[uniffi::export]
pub fn parse_makima_command(line: &str) -> Result<MakimaCommand, InputError> {
    Ok(match tui::parse_command(line)? {
        tui::Command::New {
            name,
            lead,
            children,
            child,
        } => MakimaCommand::New {
            name,
            lead,
            children,
            child,
        },
        tui::Command::Add { preset, count } => MakimaCommand::Add { preset, count },
        // Help, and any command `Command` (non_exhaustive) gains later,
        // which this bridge doesn't know how to run.
        _ => MakimaCommand::Help,
    })
}

// MARK: - Keys and focus

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FocusDirection {
    Left,
    Down,
    Up,
    Right,
}

/// Which key map Makima uses (`[keys] style` in the config).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum KeyStyle {
    /// Modal: NOR for Makima, INS for the pane, minor modes `g`,
    /// `space`, `C-w`.
    Helix,
    /// A prefix key, then one key, then back to the pane.
    Tmux,
}

/// What a key sequence does (see `makima_tui::Action`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum KeyAction {
    Insert,
    Focus {
        direction: FocusDirection,
    },
    NextAgent,
    PreviousAgent,
    NextWork,
    PreviousWork,
    Work {
        number: u8,
    },
    WorkPicker,
    NewWork,
    NewChild,
    Zoom,
    TalkToAll,
    Command,
    Close,
    Voice,
    Help,
    /// Open or hide tool `index` (in the order given to the engine).
    Tool {
        index: u16,
    },
}

/// A tool as the key map sees it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ToolKeyInfo {
    /// One character, or `None` for the tools menu only.
    pub key: Option<String>,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct KeyBinding {
    /// e.g. `g n` or `C-w o`.
    pub keys: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum KeyOutcome {
    Action {
        action: KeyAction,
    },
    /// A minor mode opened; the chrome shows its keys until the next key.
    Pending,
    /// Esc: nothing pending any more.
    Cancelled,
    /// Not a key in this map.
    Unbound,
}

/// A key map and the minor mode it's waiting in. Keys are named like
/// Helix's: `h`, `G`, `space`, `enter`, `esc`, `left`, `C-w`.
#[derive(Debug, uniffi::Object)]
pub struct KeymapEngine(Mutex<tui::Keymap>);

#[uniffi::export]
impl KeymapEngine {
    #[uniffi::constructor]
    #[must_use]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "UniFFI passes records by value"
    )]
    /// `pane_keys` adds `h j k l` for moving between agents, for when
    /// Ctrl+h/j/k/l is off.
    pub fn new(style: KeyStyle, tools: Vec<ToolKeyInfo>, pane_keys: bool) -> Arc<Self> {
        let tools = tools
            .iter()
            .map(|t| {
                tui::ToolKey::new(
                    t.key.as_deref().and_then(|k| k.chars().next()),
                    t.label.clone(),
                )
            })
            .collect();
        Arc::new(Self(Mutex::new(
            tui::Keymap::new(style.into())
                .with_tools(tools)
                .with_pane_keys(pane_keys),
        )))
    }

    #[must_use]
    pub fn style(&self) -> KeyStyle {
        self.keymap().style().into()
    }

    /// Applies one key.
    #[must_use]
    pub fn feed(&self, key: &str) -> KeyOutcome {
        match self.keymap().feed(key) {
            tui::KeyOutcome::Action(action) => KeyAction::try_from(action)
                .map_or(KeyOutcome::Unbound, |action| KeyOutcome::Action { action }),
            tui::KeyOutcome::Pending(_) => KeyOutcome::Pending,
            tui::KeyOutcome::Cancelled => KeyOutcome::Cancelled,
            // Unbound, and any outcome added later.
            _ => KeyOutcome::Unbound,
        }
    }

    /// Whether a minor mode is waiting for its next key.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.keymap().is_pending()
    }

    /// Forgets any minor mode.
    pub fn reset(&self) {
        self.keymap().reset();
    }

    /// Every key sequence and what it does.
    #[must_use]
    pub fn help(&self) -> Vec<KeyBinding> {
        self.keymap()
            .help()
            .into_iter()
            .map(|(keys, label)| KeyBinding { keys, label })
            .collect()
    }
}

impl KeymapEngine {
    /// A panic mid-key leaves at worst a stale minor mode, so poisoning
    /// is ignored.
    fn keymap(&self) -> MutexGuard<'_, tui::Keymap> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl From<KeyStyle> for tui::KeyStyle {
    fn from(style: KeyStyle) -> Self {
        match style {
            KeyStyle::Helix => Self::Helix,
            KeyStyle::Tmux => Self::Tmux,
        }
    }
}

impl From<tui::KeyStyle> for KeyStyle {
    fn from(style: tui::KeyStyle) -> Self {
        match style {
            tui::KeyStyle::Tmux => Self::Tmux,
            // Helix, and any style added to the config later.
            _ => Self::Helix,
        }
    }
}

/// Fails for an action this bridge doesn't know yet.
impl TryFrom<tui::Action> for KeyAction {
    type Error = UnknownToBridge;

    fn try_from(action: tui::Action) -> Result<Self, Self::Error> {
        Ok(match action {
            tui::Action::Insert => Self::Insert,
            tui::Action::Focus(direction) => Self::Focus {
                direction: direction.into(),
            },
            tui::Action::NextAgent => Self::NextAgent,
            tui::Action::PreviousAgent => Self::PreviousAgent,
            tui::Action::NextWork => Self::NextWork,
            tui::Action::PreviousWork => Self::PreviousWork,
            tui::Action::Work(number) => Self::Work { number },
            tui::Action::WorkPicker => Self::WorkPicker,
            tui::Action::NewWork => Self::NewWork,
            tui::Action::NewChild => Self::NewChild,
            tui::Action::Zoom => Self::Zoom,
            tui::Action::TalkToAll => Self::TalkToAll,
            tui::Action::Command => Self::Command,
            tui::Action::Close => Self::Close,
            tui::Action::Voice => Self::Voice,
            tui::Action::Help => Self::Help,
            tui::Action::Tool(index) => Self::Tool { index },
            other => return Err(UnknownToBridge(format!("{other:?}"))),
        })
    }
}

// MARK: - Picker

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PickerItemInfo {
    /// Returned when chosen.
    pub id: String,
    pub label: String,
    /// Shown dimmed after the label.
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum PickerEvent {
    Chosen { id: String },
    Cancelled,
}

/// A fuzzy-filtered list drawn over the chrome, fed raw key bytes.
#[derive(Debug, uniffi::Object)]
pub struct Picker(Mutex<tui::Picker>);

#[uniffi::export]
impl Picker {
    #[uniffi::constructor]
    #[must_use]
    pub fn new(title: &str, items: Vec<PickerItemInfo>) -> Arc<Self> {
        let items = items
            .into_iter()
            .map(|i| tui::PickerItem::new(i.id, i.label, i.detail))
            .collect();
        Arc::new(Self(Mutex::new(tui::Picker::new(title, items))))
    }

    /// Applies one chunk of terminal input.
    #[must_use]
    pub fn feed(&self, input: &[u8]) -> Option<PickerEvent> {
        Some(match self.picker().feed(input)? {
            tui::PickerEvent::Chosen(id) => PickerEvent::Chosen { id },
            // Cancelled, and any event added later.
            _ => PickerEvent::Cancelled,
        })
    }
}

impl Picker {
    fn picker(&self) -> MutexGuard<'_, tui::Picker> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl From<tui::Direction> for FocusDirection {
    fn from(direction: tui::Direction) -> Self {
        match direction {
            tui::Direction::Left => Self::Left,
            tui::Direction::Down => Self::Down,
            tui::Direction::Up => Self::Up,
            tui::Direction::Right => Self::Right,
        }
    }
}

/// The pane next to `from` in `direction`, from the last frame's rects.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI passes records by value"
)]
#[must_use]
pub fn neighbor_pane(
    panes: Vec<ChromePaneRect>,
    from: AgentId,
    direction: FocusDirection,
) -> Option<AgentId> {
    let rects: Vec<_> = panes
        .iter()
        .map(|p| tui::PaneRect::new(p.id, p.x, p.y, p.width, p.height))
        .collect();
    let direction = match direction {
        FocusDirection::Left => tui::Direction::Left,
        FocusDirection::Down => tui::Direction::Down,
        FocusDirection::Up => tui::Direction::Up,
        FocusDirection::Right => tui::Direction::Right,
    };
    tui::neighbor(&rects, from, direction)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use super::*;

    #[test]
    fn renders_and_returns_typed_pane_ids() {
        let lead = AgentId::new();
        let frame = render_chrome(
            ChromeState {
                tabs: vec![ChromeTab {
                    label: "auth".into(),
                    active: true,
                    attention: false,
                }],
                panes: vec![ChromePane {
                    id: lead,
                    name: "claude".into(),
                    role: ChromePaneRole::Lead,
                    status: ChromePaneStatus::Working,
                    detail: String::new(),
                    focused: true,
                    unguarded: false,
                }],
                zoomed: None,
                mode: ChromeMode::Pane,
                talk: ChromeTalkLine {
                    prompt: String::new(),
                    text: String::new(),
                    cursor: 0,
                },
                counts: ChromeCounts::default(),
                work_name: Some("auth".into()),
                clock: "09:00".into(),
                show_help: false,
                confirm: None,
                notice: None,
                float: None,
                leader: "^Space".into(),
            },
            KeymapEngine::new(KeyStyle::Helix, Vec::new(), false),
            None,
            None,
            100,
            30,
        );
        assert_eq!(frame.panes.len(), 1);
        assert_eq!(frame.panes[0].id, lead);
        assert!(frame.ansi.contains("lead"));
    }

    #[test]
    fn routes_and_parses() {
        let lead = TalkRecipient {
            id: AgentId::new(),
            name: "claude".into(),
        };
        let routed =
            route_talk("@all hi", TalkTarget::Lead, Some(lead.clone()), Vec::new()).unwrap();
        assert_eq!(routed.recipients, vec![lead.id]);
        assert!(matches!(
            parse_makima_command("new x --children 2"),
            Ok(MakimaCommand::New { children: 2, .. })
        ));
        assert!(matches!(
            parse_makima_command("nope"),
            Err(InputError::UnknownCommand { .. })
        ));
    }

    #[test]
    fn keymap_and_picker_round_trip() {
        let keys = KeymapEngine::new(KeyStyle::Helix, Vec::new(), false);
        assert_eq!(keys.feed("space"), KeyOutcome::Pending);
        assert!(keys.is_pending());
        assert_eq!(
            keys.feed("b"),
            KeyOutcome::Action {
                action: KeyAction::WorkPicker
            }
        );
        assert!(keys.help().iter().any(|b| b.keys == "space b"));

        let picker = Picker::new(
            "work",
            vec![
                PickerItemInfo {
                    id: "a".into(),
                    label: "auth".into(),
                    detail: String::new(),
                },
                PickerItemInfo {
                    id: "b".into(),
                    label: "billing".into(),
                    detail: String::new(),
                },
            ],
        );
        assert_eq!(picker.feed(b"bil"), None);
        assert_eq!(
            picker.feed(b"\r"),
            Some(PickerEvent::Chosen { id: "b".into() })
        );
    }

    #[test]
    fn editor_round_trip() {
        let editor = TalkEditor::new();
        assert!(editor.feed(b"hi").is_empty());
        assert_eq!(editor.cursor(), 2);
        assert_eq!(
            editor.feed(b"\r"),
            vec![TalkEditorEvent::Submitted { line: "hi".into() }]
        );
    }
}
