//! Key maps: what keys do when Makima, not the agent's terminal, has them.
//!
//! Two styles:
//!
//! - **Helix** (default): modal. `Ctrl+Space` switches from INS (keys go to
//!   the pane) to NOR, where keys are commands until `i`. Helix's own minor
//!   modes carry over: `g` goto (`gn`/`gp` next/previous work), `Space`
//!   (`b` the work picker), `Ctrl-w` window mode, `:` commands. A key that
//!   opens a minor mode shows its keys in a popup, as Helix's infobox does.
//! - **tmux**: `Ctrl+Space` is a prefix; one key after it runs an action and
//!   returns to the pane.
//!
//! Tools (lazygit, a shell, …) come from the config, so their keys are added
//! at runtime: `space g` / `^Space g` open one directly, and `space o` /
//! `^Space T` list them all.
//!
//! The help overlay is generated from the same tables, so it can't drift.

/// A direction for moving focus between panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Down,
    Up,
    Right,
}

pub use makima_core::config::KeyStyle;

/// What a key sequence does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Action {
    /// Back to typing into the focused pane (Helix `i`).
    Insert,
    Focus(Direction),
    NextAgent,
    PreviousAgent,
    NextWork,
    PreviousWork,
    /// Jump to the work in tab `n` (1-based).
    Work(u8),
    /// Fuzzy-pick a work (Helix `Space b`).
    WorkPicker,
    NewWork,
    NewChild,
    Zoom,
    TalkToAll,
    Command,
    Close,
    Voice,
    Help,
    /// Open or hide tool `n` (in the order the tools were given).
    Tool(u16),
}

/// A minor mode's keys, shown in a popup while it waits for the next key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct KeyMenu {
    pub title: String,
    /// Key and what it does, in display order.
    pub entries: Vec<(String, String)>,
}

/// The result of one key.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeyOutcome {
    Action(Action),
    /// A minor mode opened; show its keys.
    Pending(KeyMenu),
    /// Esc: nothing pending any more.
    Cancelled,
    /// Not a key in this map.
    Unbound,
}

#[derive(Debug, Clone, Copy)]
enum Node {
    Action(Action, &'static str),
    Menu(&'static str, &'static [(&'static str, Node)]),
    /// The tools menu, built from the configured tools.
    Tools,
}

use Action as A;
use Direction as D;

/// NOR's work menu: everything about works and agents as a whole.
const HELIX_SPACE: &[(&str, Node)] = &[
    ("b", Node::Action(A::WorkPicker, "pick work")),
    ("n", Node::Action(A::NewWork, "new work")),
    ("c", Node::Action(A::NewChild, "new child")),
    ("a", Node::Action(A::TalkToAll, "message everyone")),
    ("v", Node::Action(A::Voice, "speak")),
    ("x", Node::Action(A::Close, "close agent")),
    ("z", Node::Action(A::Zoom, "zoom agent")),
    ("o", Node::Tools),
];

/// NOR: tools (their own keys, added at runtime), works, and the way back
/// to the agent. Moving between agents isn't here: that's Ctrl+h/j/k/l
/// while typing (see [`PANE_KEYS`]).
const HELIX: &[(&str, Node)] = &[
    ("i", Node::Action(A::Insert, "back to the agent")),
    ("a", Node::Action(A::Insert, "back to the agent")),
    ("enter", Node::Action(A::Insert, "back to the agent")),
    ("[", Node::Action(A::PreviousWork, "previous work")),
    ("]", Node::Action(A::NextWork, "next work")),
    ("space", Node::Menu("space", HELIX_SPACE)),
    (":", Node::Action(A::Command, "command")),
    ("?", Node::Action(A::Help, "all keys")),
];

const TMUX: &[(&str, Node)] = &[
    ("c", Node::Action(A::NewChild, "new child")),
    ("n", Node::Action(A::NewWork, "new work")),
    ("1", Node::Action(A::Work(1), "work 1")),
    ("2", Node::Action(A::Work(2), "work 2")),
    ("3", Node::Action(A::Work(3), "work 3")),
    ("4", Node::Action(A::Work(4), "work 4")),
    ("5", Node::Action(A::Work(5), "work 5")),
    ("6", Node::Action(A::Work(6), "work 6")),
    ("7", Node::Action(A::Work(7), "work 7")),
    ("8", Node::Action(A::Work(8), "work 8")),
    ("9", Node::Action(A::Work(9), "work 9")),
    ("[", Node::Action(A::PreviousWork, "previous work")),
    ("]", Node::Action(A::NextWork, "next work")),
    ("w", Node::Action(A::WorkPicker, "pick work")),
    ("z", Node::Action(A::Zoom, "zoom agent")),
    ("a", Node::Action(A::TalkToAll, "message everyone")),
    (":", Node::Action(A::Command, "command")),
    ("x", Node::Action(A::Close, "close agent")),
    ("v", Node::Action(A::Voice, "speak")),
    ("T", Node::Tools),
    ("?", Node::Action(A::Help, "all keys")),
];

/// Moving between agents, only when `[keys] navigate = "off"` takes
/// Ctrl+h/j/k/l away (see [`Keymap::with_pane_keys`]).
const PANE_KEYS: &[(&str, Node)] = &[
    ("h", Node::Action(A::Focus(D::Left), "focus left")),
    ("j", Node::Action(A::Focus(D::Down), "focus down")),
    ("k", Node::Action(A::Focus(D::Up), "focus up")),
    ("l", Node::Action(A::Focus(D::Right), "focus right")),
    ("left", Node::Action(A::Focus(D::Left), "focus left")),
    ("down", Node::Action(A::Focus(D::Down), "focus down")),
    ("up", Node::Action(A::Focus(D::Up), "focus up")),
    ("right", Node::Action(A::Focus(D::Right), "focus right")),
    ("O", Node::Action(A::PreviousAgent, "previous agent")),
];

/// Keys hidden from help because another entry already covers them.
const HELP_HIDDEN: &[&str] = &[
    "a", "enter", "left", "down", "up", "right", "2", "3", "4", "5", "6", "7", "8",
];

/// A tool as the key map sees it: its own key, if any, and its label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolKey {
    pub key: Option<char>,
    pub label: String,
}

impl ToolKey {
    #[must_use]
    pub fn new(key: Option<char>, label: impl Into<String>) -> Self {
        Self {
            key,
            label: label.into(),
        }
    }
}

/// What a key map is waiting in.
#[derive(Debug, Clone, Copy)]
enum Pending {
    Menu(&'static str, &'static [(&'static str, Node)]),
    Tools,
}

/// A key map and any minor mode it's waiting in.
#[derive(Debug, Clone)]
pub struct Keymap {
    style: KeyStyle,
    pending: Option<Pending>,
    tools: Vec<ToolKey>,
    /// `h j k l` move between agents (when Ctrl+h/j/k/l is off).
    pane_keys: bool,
}

impl Keymap {
    #[must_use]
    pub fn new(style: KeyStyle) -> Self {
        Self {
            style,
            pending: None,
            tools: Vec::new(),
            pane_keys: false,
        }
    }

    /// Adds `h j k l` (and arrows) for moving between agents, for when
    /// Ctrl+h/j/k/l is turned off.
    #[must_use]
    pub fn with_pane_keys(mut self, pane_keys: bool) -> Self {
        self.pane_keys = pane_keys;
        self
    }

    /// Adds tools: their keys open them directly from the space menu
    /// (Helix) or after the prefix (tmux), and the tools menu lists all.
    #[must_use]
    pub fn with_tools(mut self, tools: Vec<ToolKey>) -> Self {
        self.tools = tools;
        self
    }

    #[must_use]
    pub fn style(&self) -> KeyStyle {
        self.style
    }

    /// Whether a minor mode is waiting for its next key.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// The keys of the minor mode waiting for its next key, if any.
    #[must_use]
    pub fn menu(&self) -> Option<KeyMenu> {
        match self.pending? {
            Pending::Menu(title, entries) => {
                let mut items: Vec<(String, String)> = entries
                    .iter()
                    .map(|(k, node)| ((*k).to_owned(), label(*node).to_owned()))
                    .collect();
                if self.takes_tool_keys(self.pending) {
                    items.extend(
                        self.tools
                            .iter()
                            .filter_map(|t| Some((t.key?.to_string(), t.label.clone()))),
                    );
                }
                Some(KeyMenu {
                    title: title.to_owned(),
                    entries: items,
                })
            }
            Pending::Tools => Some(KeyMenu {
                title: "tools".to_owned(),
                entries: self
                    .tools_menu()
                    .into_iter()
                    .map(|(key, index)| (key.to_string(), self.tools[index].label.clone()))
                    .collect(),
            }),
        }
    }

    /// Forgets any minor mode.
    pub fn reset(&mut self) {
        self.pending = None;
    }

    /// Applies one key, named like Helix's key map: `h`, `G`, `space`,
    /// `enter`, `esc`, `left`, `C-w` (Ctrl+w).
    pub fn feed(&mut self, key: &str) -> KeyOutcome {
        let pending = self.pending.take();
        if key == "esc" {
            return KeyOutcome::Cancelled;
        }
        let tool_keys = self.takes_tool_keys(pending);
        let table = match pending {
            Some(Pending::Tools) => {
                let mut chars = key.chars();
                let chosen = match (chars.next(), chars.next()) {
                    (Some(c), None) => self.tools_menu().into_iter().find(|(k, _)| *k == c),
                    _ => None,
                };
                return chosen.map_or(KeyOutcome::Unbound, |(_, index)| Self::tool_action(index));
            }
            Some(Pending::Menu(_, entries)) => entries,
            None => self.root(),
        };
        if tool_keys && let Some(index) = self.tool_with_key(key) {
            return Self::tool_action(index);
        }
        let at_root = pending.is_none();
        let found = table.iter().find(|(k, _)| *k == key).or_else(|| {
            (at_root && self.pane_keys)
                .then(|| PANE_KEYS.iter().find(|(k, _)| *k == key))
                .flatten()
        });
        match found.map(|(_, node)| *node) {
            Some(Node::Action(action, _)) => KeyOutcome::Action(action),
            Some(Node::Menu(title, entries)) => self.open(Pending::Menu(title, entries)),
            Some(Node::Tools) => self.open(Pending::Tools),
            None => KeyOutcome::Unbound,
        }
    }

    fn open(&mut self, pending: Pending) -> KeyOutcome {
        self.pending = Some(pending);
        self.menu().map_or(KeyOutcome::Unbound, KeyOutcome::Pending)
    }

    fn tool_action(index: usize) -> KeyOutcome {
        u16::try_from(index).map_or(KeyOutcome::Unbound, |i| KeyOutcome::Action(Action::Tool(i)))
    }

    fn tool_with_key(&self, key: &str) -> Option<usize> {
        let mut chars = key.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            return None;
        };
        self.tools.iter().position(|t| t.key == Some(c))
    }

    /// The tools menu: each tool under its own key, or the next free digit.
    fn tools_menu(&self) -> Vec<(char, usize)> {
        let mut digits = ('1'..='9').filter(|d| !self.tools.iter().any(|t| t.key == Some(*d)));
        self.tools
            .iter()
            .enumerate()
            .filter_map(|(i, t)| Some((t.key.or_else(|| digits.next())?, i)))
            .collect()
    }

    /// Every key sequence and what it does, for the help overlay.
    #[must_use]
    pub fn help(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let prefix = if self.style == KeyStyle::Tmux {
            ""
        } else {
            "space "
        };
        for tool in &self.tools {
            if let Some(key) = tool.key {
                out.push((format!("{prefix}{key}"), tool.label.clone()));
            }
        }
        collect_help(self.root(), "", &mut out);
        if self.pane_keys {
            collect_help(PANE_KEYS, "", &mut out);
        }
        out
    }

    fn root(&self) -> &'static [(&'static str, Node)] {
        match self.style {
            KeyStyle::Tmux => TMUX,
            // Helix, and any style added to the config later.
            _ => HELIX,
        }
    }

    /// Whether tools' own keys work in `pending`: in the space menu
    /// (Helix: `space g`), or right after the prefix (tmux).
    fn takes_tool_keys(&self, pending: Option<Pending>) -> bool {
        match (self.style, pending) {
            (KeyStyle::Tmux, None) => true,
            (KeyStyle::Tmux, _) => false,
            (_, Some(Pending::Menu(title, _))) => title == "space",
            _ => false,
        }
    }
}

fn label(node: Node) -> &'static str {
    match node {
        Node::Action(_, label) => label,
        Node::Menu(title, _) => title,
        Node::Tools => "tools",
    }
}

fn collect_help(
    table: &'static [(&'static str, Node)],
    prefix: &str,
    out: &mut Vec<(String, String)>,
) {
    for (key, node) in table {
        if prefix.is_empty() && HELP_HIDDEN.contains(key) {
            continue;
        }
        let keys = if prefix.is_empty() {
            (*key).to_owned()
        } else {
            format!("{prefix} {key}")
        };
        match node {
            Node::Action(_, label) => out.push((keys, (*label).to_owned())),
            // Submenus reached from another menu are listed where they live.
            Node::Menu(_, entries) if prefix.is_empty() => collect_help(entries, &keys, out),
            Node::Menu(title, _) => out.push((keys, (*title).to_owned())),
            Node::Tools => out.push((keys, "tools".to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Vec<ToolKey> {
        vec![
            ToolKey::new(Some('g'), "lazygit"),
            ToolKey::new(Some('s'), "shell"),
            ToolKey::new(None, "watch tests"),
        ]
    }

    #[test]
    fn nor_opens_tools_and_works_not_agents() {
        let mut keys = Keymap::new(KeyStyle::Helix).with_tools(tools());
        keys.feed("space");
        assert_eq!(keys.feed("g"), KeyOutcome::Action(Action::Tool(0)));
        keys.feed("space");
        assert_eq!(keys.feed("s"), KeyOutcome::Action(Action::Tool(1)));
        assert_eq!(
            keys.feed("g"),
            KeyOutcome::Unbound,
            "tools live under space"
        );
        assert_eq!(keys.feed("]"), KeyOutcome::Action(Action::NextWork));
        assert_eq!(keys.feed("i"), KeyOutcome::Action(Action::Insert));
        for key in ["h", "j", "k", "l", "C-w"] {
            assert_eq!(
                keys.feed(key),
                KeyOutcome::Unbound,
                "{key}: agents are Ctrl+hjkl's"
            );
        }

        let KeyOutcome::Pending(space) = keys.feed("space") else {
            panic!("space opens its menu")
        };
        assert!(
            space
                .entries
                .contains(&("g".to_owned(), "lazygit".to_owned()))
        );
        let KeyOutcome::Pending(menu) = keys.feed("o") else {
            panic!("space o lists every tool")
        };
        assert_eq!(
            menu.entries,
            [("g", "lazygit"), ("s", "shell"), ("1", "watch tests")]
                .map(|(k, l)| (k.to_owned(), l.to_owned()))
        );
        assert_eq!(keys.feed("1"), KeyOutcome::Action(Action::Tool(2)));

        keys.feed("space");
        assert_eq!(keys.feed("b"), KeyOutcome::Action(Action::WorkPicker));
        keys.feed("space");
        assert_eq!(keys.feed("esc"), KeyOutcome::Cancelled);
    }

    #[test]
    fn pane_keys_come_back_when_ctrl_hjkl_is_off() {
        let mut keys = Keymap::new(KeyStyle::Helix).with_pane_keys(true);
        assert_eq!(
            keys.feed("j"),
            KeyOutcome::Action(Action::Focus(Direction::Down))
        );
        assert!(
            keys.help()
                .contains(&("k".to_owned(), "focus up".to_owned()))
        );
    }

    #[test]
    fn tmux_prefix_keys() {
        let mut keys = Keymap::new(KeyStyle::Tmux).with_tools(tools());
        assert_eq!(keys.feed("3"), KeyOutcome::Action(Action::Work(3)));
        assert_eq!(keys.feed("z"), KeyOutcome::Action(Action::Zoom));
        assert_eq!(keys.feed("g"), KeyOutcome::Action(Action::Tool(0)));
        assert_eq!(keys.feed("h"), KeyOutcome::Unbound);
        assert!(matches!(keys.feed("T"), KeyOutcome::Pending(_)));
    }

    #[test]
    fn help_lists_tools_and_keys() {
        let help = Keymap::new(KeyStyle::Helix).with_tools(tools()).help();
        let has = |keys: &str, what: &str| help.contains(&(keys.to_owned(), what.to_owned()));
        assert!(has("space g", "lazygit"));
        assert!(has("space o", "tools"));
        assert!(has("space b", "pick work"));
        assert!(has("] ", "next work") || has("]", "next work"));
        assert!(
            !help.iter().any(|(k, _)| k == "h"),
            "no pane keys by default"
        );
    }
}
