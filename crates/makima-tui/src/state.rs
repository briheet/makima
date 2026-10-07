//! What the chrome shows. Exhaustive on purpose: the host builds these field
//! by field every frame, so a new field should be a compile error there.

use makima_core::AgentId;

use crate::keys::{KeyMenu, KeyStyle};

/// One piece of work in the tab line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabSpec {
    pub label: String,
    pub active: bool,
    /// An agent in this work needs input.
    pub attention: bool,
}

/// The lead or a numbered child.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneRole {
    Lead,
    /// Children are numbered from 1, in the order they joined.
    Child(u16),
}

/// What a pane's agent is doing, as the chrome shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneStatus {
    Starting,
    Working,
    NeedsInput,
    Done,
    Failed,
    /// The terminal closed; it can be restarted.
    Exited,
}

/// One agent's pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneSpec {
    /// Returned in [`crate::PaneRect`].
    pub id: AgentId,
    pub name: String,
    pub role: PaneRole,
    pub status: PaneStatus,
    /// What's running (the terminal title), shown after the name.
    pub detail: String,
    pub focused: bool,
    /// Started with permission checks off (bypass); flagged in red.
    pub unguarded: bool,
}

/// Which input mode the chrome is in; shown as the status-line badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromeMode {
    /// Keys go to the focused pane (Helix's INS).
    Pane,
    /// Keys are Makima commands (Helix's NOR; tmux's prefix).
    Keys,
    /// Picking from a fuzzy list (e.g. switching work).
    Picker,
    /// Typing into the talk line.
    Talk,
    /// Typing a `makima>` command.
    Command,
    /// Waiting for y/n.
    Confirm,
    /// Starting new work: the welcome screen, templates and form.
    NewWork,
    /// Keys go to a tool's floating window.
    Tool,
}

/// A tool open in a floating window over the panes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FloatSpec {
    /// The tool's label, e.g. `lazygit`.
    pub title: String,
    /// Where it runs, shown after the title.
    pub folder: String,
    /// Share of the body, in percent (30 to 100).
    pub percent: u8,
    /// Fill the whole body.
    pub zoomed: bool,
    /// How to hide it, shown at the right of its border.
    pub hint: String,
}

impl FloatSpec {
    #[must_use]
    pub fn new(title: impl Into<String>, folder: impl Into<String>, percent: u8) -> Self {
        Self {
            title: title.into(),
            folder: folder.into(),
            percent,
            zoomed: false,
            hint: String::new(),
        }
    }

    #[must_use]
    pub fn zoomed(mut self, zoomed: bool) -> Self {
        self.zoomed = zoomed;
        self
    }

    #[must_use]
    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = hint.into();
        self
    }
}

/// The line above the status bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TalkLine {
    /// `@all ▸ `, `makima> `, … Empty when idle.
    pub prompt: String,
    pub text: String,
    /// Cursor position in characters within `text`.
    pub cursor: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StatusCounts {
    pub working: u16,
    pub waiting: u16,
    pub done: u16,
    pub failed: u16,
}

/// Everything the chrome shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChromeState {
    pub tabs: Vec<TabSpec>,
    /// The selected work's agents, lead first.
    pub panes: Vec<PaneSpec>,
    /// A pane filling the body, by id.
    pub zoomed: Option<AgentId>,
    pub mode: ChromeMode,
    pub talk: TalkLine,
    pub counts: StatusCounts,
    /// The selected work's name.
    pub work_name: Option<String>,
    /// Shown at the right of the status line, e.g. `14:02`.
    pub clock: String,
    pub show_help: bool,
    /// A y/n question shown in the status line in [`ChromeMode::Confirm`].
    pub confirm: Option<String>,
    /// A one-line message (an error, a hint) shown in the talk line.
    pub notice: Option<String>,
    /// Which key map is active; changes the mode badge and hints.
    pub key_style: KeyStyle,
    /// A minor mode waiting for its next key (Helix's infobox).
    pub menu: Option<KeyMenu>,
    /// The help overlay's key list; the bare key map's when empty.
    pub bindings: Vec<(String, String)>,
    /// A tool open over the panes.
    pub float: Option<FloatSpec>,
    /// How the key into Makima's keys is written in hints, e.g. `^Space`.
    pub leader: String,
}
