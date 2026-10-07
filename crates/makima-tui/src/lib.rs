//! Makima's terminal chrome, as a pure TUI.
//!
//! The app's window is a terminal: a tab line, box-drawn panes with their
//! titles in the border, a talk line and a status line. This crate turns a
//! [`ChromeState`] into the exact bytes a terminal needs to draw it (see
//! [`render`]), plus where each agent's live terminal goes. The host feeds
//! those bytes to a terminal surface and places the agents' surfaces in the
//! returned [`PaneRect`]s.
//!
//! It also owns the chrome's input: the talk line's [`LineEditor`], routing
//! `@target` messages ([`route_talk`]), the `makima>` command language
//! ([`parse_command`]), the key maps ([`Keymap`], Helix or tmux style), the
//! fuzzy [`Picker`], and the new-work flow's single-page form, built from
//! pluggable fields.
//!
//! Everything is synchronous and free of I/O.

mod ansi;
mod command;
mod editor;
mod flow;
mod form;
mod fuzzy;
mod input;
mod keys;
mod layout;
mod picker;
mod render;
mod state;
mod talk;

pub use command::{Command, CommandError, parse_command};
pub use editor::{EditorEvent, LineEditor};
pub use flow::{
    ChildRequest, FlowEvent, FlowOptions, NewWorkFlow, NewWorkRequest, RecentWork, VoiceClip,
};
pub use input::{Key, parse_keys};
pub use keys::{Action, Direction, KeyMenu, KeyOutcome, KeyStyle, Keymap, ToolKey};
pub use layout::neighbor;
pub use picker::{Picker, PickerEvent, PickerItem};
pub use render::{CellRect, Frame, Overlay, PaneRect, TabHit, render};
pub use state::{
    ChromeMode, ChromeState, FloatSpec, PaneRole, PaneSpec, PaneStatus, StatusCounts, TabSpec,
    TalkLine,
};
pub use talk::{Recipient, Routed, TalkError, Target, route_talk};
