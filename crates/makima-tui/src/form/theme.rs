use ratatui::style::{Color, Modifier, Style};

use crate::render::{ACCENT, MUTED};

/// How forms look. Colours are ANSI palette entries, so they follow the
/// terminal theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// The focused field's bar and title, a selected option's marker.
    pub accent: Style,
    /// Blurred titles, descriptions, placeholders.
    pub muted: Style,
    /// Values and the chosen option.
    pub value: Style,
    /// The chosen option while its field has focus.
    pub chosen: Style,
    /// Group headings.
    pub heading: Style,
    pub error: Style,
    /// A focused button.
    pub button: Style,
    /// The focused field's left bar.
    pub bar: char,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Style::new().fg(ACCENT),
            muted: Style::new().fg(MUTED),
            value: Style::new(),
            chosen: Style::new()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
            heading: Style::new().fg(MUTED).add_modifier(Modifier::BOLD),
            error: Style::new().fg(Color::Red),
            button: Style::new()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
            bar: '┃',
        }
    }
}
