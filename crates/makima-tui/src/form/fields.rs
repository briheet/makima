//! The built-in fields.

use ratatui::text::Span;

use super::{Field, FieldOutcome, FieldView, Focus, Theme, Value};
use crate::editor::LineEditor;
use crate::input::Key;

/// A line of text, with a dim placeholder while empty. Stacked inputs
/// wrap long text over several rows.
#[derive(Debug)]
pub struct TextInput {
    key: String,
    title: String,
    description: String,
    placeholder: String,
    editor: LineEditor,
    stacked: bool,
}

/// The most rows a stacked input shows; longer text scrolls.
const MAX_ROWS: usize = 4;

impl TextInput {
    #[must_use]
    pub fn new(key: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            description: String::new(),
            placeholder: String::new(),
            editor: LineEditor::new(),
            stacked: false,
        }
    }

    /// Title on its own row, the text beneath it, wrapping.
    #[must_use]
    pub fn stacked(mut self) -> Self {
        self.stacked = true;
        self
    }

    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    #[must_use]
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    #[must_use]
    pub fn with_text(mut self, text: &str) -> Self {
        self.editor.set_text(text);
        self
    }
}

impl TextInput {
    /// `text` in rows of `room` characters, at most [`MAX_ROWS`], keeping
    /// the cursor's row in view.
    fn wrapped(
        &self,
        text: &str,
        room: usize,
        editing: bool,
        style: ratatui::style::Style,
    ) -> FieldView {
        let chars: Vec<char> = text.chars().collect();
        let mut rows: Vec<String> = chars.chunks(room).map(|c| c.iter().collect()).collect();
        let cursor = self.editor.cursor();
        // A cursor at the end of a full row starts the next one.
        if cursor == chars.len() && cursor.is_multiple_of(room) {
            rows.push(String::new());
        }
        let cursor_row = cursor / room;
        let first = cursor_row
            .saturating_sub(MAX_ROWS - 1)
            .min(rows.len().saturating_sub(MAX_ROWS));
        let shown: Vec<Vec<Span<'static>>> = rows
            .into_iter()
            .skip(first)
            .take(MAX_ROWS)
            .map(|row| vec![Span::styled(row, style)])
            .collect();
        let mut shown = shown.into_iter();
        FieldView {
            spans: shown.next().unwrap_or_default(),
            more: shown.collect(),
            cursor: editing.then(|| u16::try_from(cursor % room).unwrap_or(u16::MAX)),
            cursor_line: cursor_row - first,
        }
    }
}

impl Field for TextInput {
    fn key(&self) -> &str {
        &self.key
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn handle(&mut self, key: &Key) -> FieldOutcome {
        match key {
            Key::Enter | Key::Esc | Key::Tab | Key::BackTab | Key::Up | Key::Down => {
                FieldOutcome::Ignored
            }
            key => {
                self.editor.handle(key);
                FieldOutcome::Consumed
            }
        }
    }

    fn view(&self, focus: Focus, width: u16, theme: &Theme) -> FieldView {
        let text = self.editor.text();
        let editing = focus == Focus::Editing;
        if text.is_empty() {
            // While editing, the cursor sits on a space so the placeholder
            // stays readable.
            let placeholder = if editing {
                format!(" {}", self.placeholder)
            } else {
                self.placeholder.clone()
            };
            return FieldView {
                spans: vec![Span::styled(placeholder, theme.muted)],
                cursor: editing.then_some(0),
                ..FieldView::default()
            };
        }
        let room = usize::from(width).saturating_sub(1).max(1);
        // Focused but not editing (Helix's normal mode): shown selected.
        let style = if focus == Focus::Focused {
            theme.chosen
        } else {
            theme.value
        };
        if self.stacked {
            return self.wrapped(&text, room, editing, style);
        }
        let skip = self.editor.cursor().saturating_sub(room);
        let shown: String = text.chars().skip(skip).take(room).collect();
        FieldView {
            spans: vec![Span::styled(shown, style)],
            cursor: editing.then(|| u16::try_from(self.editor.cursor() - skip).unwrap_or(u16::MAX)),
            ..FieldView::default()
        }
    }

    fn hints(&self, modal: bool) -> Vec<(&'static str, &'static str)> {
        if modal {
            vec![("i", "edit"), ("c", "replace")]
        } else {
            vec![("⏎", "next")]
        }
    }

    fn takes_text(&self) -> bool {
        true
    }

    fn stacked(&self) -> bool {
        self.stacked
    }

    fn value(&self) -> Value {
        Value::Text(self.editor.text())
    }

    fn set_text(&mut self, text: &str) {
        self.editor.set_text(text);
    }

    fn append_text(&mut self, text: &str) -> bool {
        self.editor.append(text);
        true
    }
}

/// One of a fixed list. While focused every option shows, the chosen one
/// highlighted; otherwise only the chosen one.
#[derive(Debug)]
pub struct Select {
    key: String,
    title: String,
    description: String,
    options: Vec<String>,
    /// Per option; replaces `description` when set.
    option_descriptions: Vec<String>,
    selected: usize,
}

impl Select {
    #[must_use]
    pub fn new(key: impl Into<String>, title: impl Into<String>, options: Vec<String>) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            description: String::new(),
            options,
            option_descriptions: Vec::new(),
            selected: 0,
        }
    }

    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    /// One description per option, shown for the chosen one.
    #[must_use]
    pub fn with_option_descriptions(mut self, descriptions: Vec<String>) -> Self {
        self.option_descriptions = descriptions;
        self
    }

    #[must_use]
    pub fn with_selected(mut self, index: usize) -> Self {
        self.selected = index.min(self.options.len().saturating_sub(1));
        self
    }

    fn step(&mut self, by: isize) {
        let count = self.options.len().max(1);
        self.selected = self
            .selected
            .saturating_add_signed(by + count.cast_signed())
            % count;
    }
}

impl Field for Select {
    fn key(&self) -> &str {
        &self.key
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn description(&self) -> &str {
        self.option_descriptions
            .get(self.selected)
            .filter(|d| !d.is_empty())
            .unwrap_or(&self.description)
    }

    fn handle(&mut self, key: &Key) -> FieldOutcome {
        match key {
            Key::Left | Key::Char('h') => self.step(-1),
            Key::Right | Key::Char('l' | ' ') => self.step(1),
            _ => return FieldOutcome::Ignored,
        }
        FieldOutcome::Consumed
    }

    fn view(&self, focus: Focus, width: u16, theme: &Theme) -> FieldView {
        let focused = focus != Focus::Blurred;
        let chosen = self.options.get(self.selected).cloned().unwrap_or_default();
        if !focused {
            return FieldView {
                spans: vec![Span::styled(chosen, theme.value)],
                cursor: None,
                ..FieldView::default()
            };
        }
        let all: usize = self.options.iter().map(|o| o.chars().count() + 2).sum();
        if all > usize::from(width) {
            // Too many to show at once: the chosen one and where it is.
            return FieldView {
                spans: vec![
                    Span::styled(format!(" {chosen} "), theme.chosen),
                    Span::styled(
                        format!("  {}/{}", self.selected + 1, self.options.len()),
                        theme.muted,
                    ),
                ],
                cursor: None,
                ..FieldView::default()
            };
        }
        let spans = self
            .options
            .iter()
            .enumerate()
            .map(|(i, option)| {
                let style = if i == self.selected {
                    theme.chosen
                } else {
                    theme.muted
                };
                Span::styled(format!(" {option} "), style)
            })
            .collect();
        FieldView {
            spans,
            cursor: None,
            ..FieldView::default()
        }
    }

    fn hints(&self, modal: bool) -> Vec<(&'static str, &'static str)> {
        vec![(if modal { "h l" } else { "←→" }, "change")]
    }

    fn value(&self) -> Value {
        Value::Choice(self.selected)
    }
}

/// A count, with no upper limit: ←→ step it, digits type it.
#[derive(Debug)]
pub struct Number {
    key: String,
    title: String,
    description: String,
    value: u32,
    /// The next digit replaces the value rather than extending it.
    fresh: bool,
}

impl Number {
    #[must_use]
    pub fn new(key: impl Into<String>, title: impl Into<String>, value: u32) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            description: String::new(),
            value,
            fresh: true,
        }
    }

    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}

impl Field for Number {
    fn key(&self) -> &str {
        &self.key
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn focus(&mut self) {
        self.fresh = true;
    }

    fn handle(&mut self, key: &Key) -> FieldOutcome {
        match key {
            Key::Left | Key::Char('-' | 'h') => self.value = self.value.saturating_sub(1),
            Key::Right | Key::Char('+' | 'l') => self.value = self.value.saturating_add(1),
            Key::Char(c @ '0'..='9') => {
                let digit = c.to_digit(10).unwrap_or(0);
                self.value = if self.fresh {
                    digit
                } else {
                    self.value.saturating_mul(10).saturating_add(digit)
                };
            }
            Key::Backspace => self.value /= 10,
            _ => return FieldOutcome::Ignored,
        }
        self.fresh = false;
        FieldOutcome::Consumed
    }

    fn view(&self, focus: Focus, _width: u16, theme: &Theme) -> FieldView {
        let focused = focus != Focus::Blurred;
        let style = if focused { theme.chosen } else { theme.value };
        let text = if focused {
            format!(" {} ", self.value)
        } else {
            self.value.to_string()
        };
        FieldView {
            spans: vec![Span::styled(text, style)],
            cursor: None,
            ..FieldView::default()
        }
    }

    fn hints(&self, modal: bool) -> Vec<(&'static str, &'static str)> {
        vec![(if modal { "h l 0-9" } else { "←→ 0-9" }, "count")]
    }

    fn value(&self) -> Value {
        Value::Number(self.value)
    }
}

/// A value chosen elsewhere: Enter asks the owner (e.g. a folder picker).
#[derive(Debug)]
pub struct Action {
    key: String,
    title: String,
    description: String,
    value: String,
    /// What Enter does, e.g. "choose".
    verb: &'static str,
}

impl Action {
    #[must_use]
    pub fn new(
        key: impl Into<String>,
        title: impl Into<String>,
        value: impl Into<String>,
        verb: &'static str,
    ) -> Self {
        Self {
            key: key.into(),
            title: title.into(),
            description: String::new(),
            value: value.into(),
            verb,
        }
    }

    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}

impl Field for Action {
    fn key(&self) -> &str {
        &self.key
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn handle(&mut self, key: &Key) -> FieldOutcome {
        if *key == Key::Enter {
            FieldOutcome::Activated
        } else {
            FieldOutcome::Ignored
        }
    }

    fn view(&self, focus: Focus, _width: u16, theme: &Theme) -> FieldView {
        let focused = focus != Focus::Blurred;
        let mut spans = vec![Span::styled(self.value.clone(), theme.value)];
        if focused {
            spans.push(Span::styled(format!("   ⏎ {}", self.verb), theme.accent));
        }
        FieldView {
            spans,
            cursor: None,
            ..FieldView::default()
        }
    }

    fn hints(&self, _modal: bool) -> Vec<(&'static str, &'static str)> {
        vec![("⏎", self.verb)]
    }

    fn value(&self) -> Value {
        Value::Text(self.value.clone())
    }

    fn set_text(&mut self, text: &str) {
        text.clone_into(&mut self.value);
    }
}

/// The form's main action, e.g. start.
#[derive(Debug)]
pub struct Button {
    key: String,
    label: String,
    description: String,
}

impl Button {
    #[must_use]
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            description: String::new(),
        }
    }

    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}

impl Field for Button {
    fn key(&self) -> &str {
        &self.key
    }

    #[expect(
        clippy::unnecessary_literal_bound,
        reason = "the trait returns borrowed titles"
    )]
    fn title(&self) -> &str {
        ""
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn handle(&mut self, key: &Key) -> FieldOutcome {
        if *key == Key::Enter {
            FieldOutcome::Activated
        } else {
            FieldOutcome::Ignored
        }
    }

    fn view(&self, focus: Focus, _width: u16, theme: &Theme) -> FieldView {
        let focused = focus != Focus::Blurred;
        let style = if focused { theme.button } else { theme.accent };
        FieldView {
            spans: vec![Span::styled(format!(" {} ⏎ ", self.label), style)],
            cursor: None,
            ..FieldView::default()
        }
    }

    fn hints(&self, _modal: bool) -> Vec<(&'static str, &'static str)> {
        vec![("⏎", "start")]
    }

    fn passes_typing(&self) -> bool {
        true
    }

    fn value(&self) -> Value {
        Value::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacked_inputs_wrap_and_follow_the_cursor() {
        let mut input = TextInput::new("q", "q").stacked();
        input.set_text("abcdefghij");
        let view = input.view(Focus::Editing, 5, &Theme::default());
        // 4 columns of text a row (one kept for the cursor).
        assert_eq!(view.spans[0].content, "abcd");
        assert_eq!(view.more.len(), 2);
        assert_eq!((view.cursor_line, view.cursor), (2, Some(2)));
    }

    #[test]
    fn numbers_have_no_cap() {
        let mut count = Number::new("n", "n", 2);
        count.focus();
        for key in [Key::Char('4'), Key::Char('0'), Key::Right] {
            count.handle(&key);
        }
        assert_eq!(count.value(), Value::Number(41));
        count.focus();
        count.handle(&Key::Char('3'));
        assert_eq!(count.value(), Value::Number(3), "a fresh digit replaces");
    }

    #[test]
    fn selects_wrap_and_describe_the_chosen_option() {
        let mut select = Select::new("m", "m", vec!["a".into(), "b".into()])
            .with_option_descriptions(vec!["first".into(), "second".into()]);
        select.handle(&Key::Left);
        assert_eq!(select.value(), Value::Choice(1));
        assert_eq!(select.description(), "second");
    }
}
