//! Single-page forms, after Charm's `huh`: a form is groups of fields, one
//! field has focus, and only that field shows its description, its options
//! and its keys. Everything else is quiet.
//!
//! Pluggable at three points:
//! - a new kind of input implements [`Field`] (see `fields.rs` for the
//!   built-in [`TextInput`], [`Select`], [`Number`], [`Action`] and
//!   [`Button`]);
//! - a new form is a list of [`Group`]s, read back by key;
//! - a new look is a [`Theme`].

mod fields;
mod theme;

use std::fmt;

use ratatui::style::Modifier;
use ratatui::text::{Line, Span};

pub use fields::{Action, Button, Number, Select, TextInput};
pub use theme::Theme;

use crate::input::Key;

/// What a field did with a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldOutcome {
    /// The field used it.
    Consumed,
    /// Not for this field; the form may use it (Enter moves on).
    Ignored,
    /// Enter on a field that asks its owner to act (pick a folder, start).
    Activated,
}

/// A field's current value, read back by its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    /// The selected option's index.
    Choice(usize),
    Number(u32),
    None,
}

/// How a field is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Blurred,
    /// Focused; in a modal form, not being edited (Helix's normal mode).
    Focused,
    /// Taking text: show the cursor.
    Editing,
}

/// What a field shows after its title, and where the cursor goes.
#[derive(Debug, Default)]
pub struct FieldView {
    pub spans: Vec<Span<'static>>,
    /// The cursor's column within its line, for text inputs with focus.
    pub cursor: Option<u16>,
    /// Lines after the first, for values that wrap.
    pub more: Vec<Vec<Span<'static>>>,
    /// Which line the cursor is on: 0 is `spans`, 1 the first of `more`.
    pub cursor_line: usize,
}

/// One input in a form.
/// `Send`, so a form can live behind the FFI's lock.
pub trait Field: fmt::Debug + Send {
    /// How the owner finds it, e.g. `folder`.
    fn key(&self) -> &str;
    /// Shown in the title column; empty for buttons.
    fn title(&self) -> &str;
    /// One line of help, shown only while focused.
    #[expect(
        clippy::unnecessary_literal_bound,
        reason = "implementations return their own text"
    )]
    fn description(&self) -> &str {
        ""
    }
    /// Called when the field gains focus.
    fn focus(&mut self) {}
    fn handle(&mut self, key: &Key) -> FieldOutcome;
    /// The value part of the row, `width` cells wide at most.
    fn view(&self, focus: Focus, width: u16, theme: &Theme) -> FieldView;
    /// The keys it takes, as (keys, what) pairs, for the help line; in a
    /// `modal` form, its normal-mode keys.
    fn hints(&self, modal: bool) -> Vec<(&'static str, &'static str)>;
    /// Whether it edits text (and so has an insert mode in a modal form).
    fn takes_text(&self) -> bool {
        false
    }
    /// Drawn as its title on one line and its value, full width, beneath
    /// (long questions with long answers).
    fn stacked(&self) -> bool {
        false
    }
    fn value(&self) -> Value;
    /// Replaces a text value (a picked folder, a name that follows it).
    fn set_text(&mut self, _text: &str) {}
    /// Adds text to a text value (a voice note); `false` if it takes none.
    fn append_text(&mut self, _text: &str) -> bool {
        false
    }
    /// Typing on it goes to the form's typing field (see
    /// [`Form::typing_into`]): true for buttons, not for fields whose keys
    /// mean something (a Space on a choice, a letter on the folder).
    fn passes_typing(&self) -> bool {
        false
    }
}

/// A titled section of fields.
#[derive(Debug)]
pub struct Group {
    /// Shown as a small heading; empty for none.
    pub title: String,
    pub fields: Vec<Box<dyn Field>>,
}

impl Group {
    #[must_use]
    pub fn new(title: impl Into<String>, fields: Vec<Box<dyn Field>>) -> Self {
        Self {
            title: title.into(),
            fields,
        }
    }
}

/// What the form wants from its owner after a key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormEvent {
    /// A field was activated (Enter on an [`Action`] or [`Button`]).
    Activated(String),
    /// Esc with nothing being edited, in a modal form: leave the form.
    Back,
}

/// Groups of fields with one focus, Tab order and per-field errors.
#[derive(Debug)]
pub struct Form {
    groups: Vec<Group>,
    /// Keys of hidden fields (skipped by focus, not drawn).
    hidden: Vec<String>,
    /// The focused field's key.
    focus: String,
    /// A field's key and what's wrong with it.
    error: Option<(String, String)>,
    /// Where typing goes when the focused field takes no text.
    typing_field: Option<String>,
    /// The field typing jumped away from; Enter goes back and presses it.
    typed_from: Option<String>,
    /// Helix-style: keys are commands (`j` `k` `i` …) until `i` starts
    /// editing a text field, and Esc stops.
    modal: bool,
    /// Editing the focused text field (modal forms only).
    inserting: bool,
}

/// How far a stacked field's value sits under its title.
const STACK_INDENT: usize = 4;

/// The widest a title column gets; longer titles are cut.
const MAX_TITLE: usize = 24;

impl Form {
    /// A form focused on its first field.
    #[must_use]
    pub fn new(groups: Vec<Group>) -> Self {
        let focus = groups
            .iter()
            .flat_map(|g| &g.fields)
            .next()
            .map(|f| f.key().to_owned())
            .unwrap_or_default();
        let mut form = Self {
            groups,
            hidden: Vec::new(),
            focus,
            error: None,
            typing_field: None,
            typed_from: None,
            modal: false,
            inserting: false,
        };
        form.focus_on(&form.focus.clone());
        form
    }

    /// The visible fields' keys, in order.
    fn order(&self) -> Vec<String> {
        self.groups
            .iter()
            .flat_map(|g| &g.fields)
            .map(|f| f.key().to_owned())
            .filter(|k| !self.hidden.contains(k))
            .collect()
    }

    #[must_use]
    pub fn focused(&self) -> &str {
        &self.focus
    }

    /// Moves focus to `key`, if it's a visible field.
    pub fn focus_on(&mut self, key: &str) {
        if !self.order().iter().any(|k| k == key) {
            return;
        }
        key.clone_into(&mut self.focus);
        if let Some(field) = self.field_mut(key) {
            field.focus();
        }
    }

    fn step(&mut self, by: isize) {
        let order = self.order();
        let Some(index) = order.iter().position(|k| *k == self.focus) else {
            if let Some(first) = order.first() {
                self.focus_on(&first.clone());
            }
            return;
        };
        let next = index.saturating_add_signed(by).min(order.len() - 1);
        self.focus_on(&order[next].clone());
    }

    /// Shows or hides a field; a hidden focused field passes focus on.
    pub fn set_hidden(&mut self, key: &str, hidden: bool) {
        let is_hidden = self.hidden.iter().any(|k| k == key);
        if hidden && !is_hidden {
            let order = self.order();
            let index = order.iter().position(|k| k == key);
            self.hidden.push(key.to_owned());
            if self.focus == key {
                let order = self.order();
                let next = index.unwrap_or(0).min(order.len().saturating_sub(1));
                if let Some(next) = order.get(next) {
                    self.focus_on(&next.clone());
                }
            }
        } else if !hidden && is_hidden {
            self.hidden.retain(|k| k != key);
        }
    }

    #[must_use]
    pub fn field(&self, key: &str) -> Option<&dyn Field> {
        self.groups
            .iter()
            .flat_map(|g| &g.fields)
            .find(|f| f.key() == key)
            .map(AsRef::as_ref)
    }

    pub fn field_mut(&mut self, key: &str) -> Option<&mut Box<dyn Field>> {
        self.groups
            .iter_mut()
            .flat_map(|g| &mut g.fields)
            .find(|f| f.key() == key)
    }

    /// The value of `key`, or [`Value::None`] if there's no such field.
    #[must_use]
    pub fn value(&self, key: &str) -> Value {
        self.field(key).map_or(Value::None, Field::value)
    }

    /// Text typed on a field that takes none (a button, a choice) goes to
    /// `key` instead, replacing what's there: start typing anywhere to
    /// name the work.
    #[must_use]
    pub fn typing_into(mut self, key: impl Into<String>) -> Self {
        self.typing_field = Some(key.into());
        self
    }

    /// Makes the form modal, like Helix: in normal mode `j`/`k` move, `h`/`l`
    /// change a choice, `i` (or `a`) edits a text field and `c` replaces
    /// it, ⏎ moves on or presses a button, and Esc leaves the form; while
    /// editing, Esc returns to normal mode and ⏎ moves on.
    #[must_use]
    pub fn modal(mut self, modal: bool) -> Self {
        self.modal = modal;
        self
    }

    #[must_use]
    pub fn is_modal(&self) -> bool {
        self.modal
    }

    /// Editing a text field (in a modal form).
    #[must_use]
    pub fn is_inserting(&self) -> bool {
        self.inserting
    }

    /// `NOR` or `INS` for a modal form's mode badge.
    #[must_use]
    pub fn mode_label(&self) -> Option<&'static str> {
        self.modal
            .then_some(if self.inserting { "INS" } else { "NOR" })
    }

    /// Starts editing the focused text field, or the typing field when the
    /// focused one takes none (`i` on the start button edits the name).
    fn begin_insert(&mut self, replace: bool) {
        let focused_takes_text = self.field(&self.focus).is_some_and(Field::takes_text);
        if !focused_takes_text {
            let Some(target) = self.typing_field.clone() else {
                return;
            };
            self.typed_from = Some(self.focus.clone());
            self.focus_on(&target);
        }
        let focus = self.focus.clone();
        if let Some(field) = self.field_mut(&focus) {
            if replace {
                field.set_text("");
            } else {
                field.handle(&Key::End);
            }
        }
        self.inserting = true;
    }

    /// Normal mode in a modal form: keys are commands.
    fn handle_normal(&mut self, key: &Key) -> Option<FormEvent> {
        match key {
            Key::Char('j') | Key::Down | Key::Tab => self.step(1),
            Key::Char('k') | Key::Up | Key::BackTab => self.step(-1),
            Key::Char('g') => {
                if let Some(first) = self.order().first().cloned() {
                    self.focus_on(&first);
                }
            }
            Key::Char('G') => {
                if let Some(last) = self.order().last().cloned() {
                    self.focus_on(&last);
                }
            }
            Key::Char('i' | 'a') => self.begin_insert(false),
            Key::Char('c') => self.begin_insert(true),
            Key::Esc => return Some(FormEvent::Back),
            key => {
                let focus = self.focus.clone();
                let outcome = self
                    .field_mut(&focus)
                    .map_or(FieldOutcome::Ignored, |f| f.handle(key));
                match outcome {
                    FieldOutcome::Activated => return Some(FormEvent::Activated(focus)),
                    FieldOutcome::Ignored if *key == Key::Enter => self.step(1),
                    // Other letters are commands Helix doesn't have here.
                    _ => {}
                }
            }
        }
        None
    }

    /// Insert mode in a modal form: keys edit the focused text field.
    fn handle_insert(&mut self, key: &Key) -> Option<FormEvent> {
        match key {
            Key::Esc => {
                self.inserting = false;
                self.typed_from = None;
            }
            Key::Enter => {
                self.inserting = false;
                // `i` on the start button, a name, ⏎: start.
                if let Some(from) = self.typed_from.take() {
                    self.focus_on(&from);
                    return self.handle_normal(key);
                }
                self.step(1);
            }
            Key::Tab | Key::Down => {
                self.inserting = false;
                self.step(1);
            }
            Key::BackTab | Key::Up => {
                self.inserting = false;
                self.step(-1);
            }
            key => {
                let focus = self.focus.clone();
                if let Some(field) = self.field_mut(&focus) {
                    field.handle(key);
                }
            }
        }
        None
    }

    /// Shows `message` under `key` and focuses it, until the next key.
    pub fn set_error(&mut self, key: &str, message: impl Into<String>) {
        self.error = Some((key.to_owned(), message.into()));
        self.focus_on(key);
    }

    /// Applies one key: ↑↓ and Tab move between fields, Enter moves on
    /// unless the field acts on it, and anything else goes to the focused
    /// field.
    pub fn handle(&mut self, key: &Key) -> Option<FormEvent> {
        self.error = None;
        if self.modal {
            return if self.inserting {
                self.handle_insert(key)
            } else {
                self.handle_normal(key)
            };
        }
        if *key == Key::Enter
            && let Some(from) = self.typed_from.take()
        {
            // Typed a name on the start button: Enter starts.
            self.focus_on(&from);
            return self.handle(key);
        }
        match key {
            Key::Up | Key::BackTab => {
                self.typed_from = None;
                self.step(-1);
            }
            Key::Down | Key::Tab => {
                self.typed_from = None;
                self.step(1);
            }
            key => {
                let focus = self.focus.clone();
                let outcome = self
                    .field_mut(&focus)
                    .map_or(FieldOutcome::Ignored, |f| f.handle(key));
                match outcome {
                    FieldOutcome::Activated => return Some(FormEvent::Activated(focus)),
                    FieldOutcome::Ignored if *key == Key::Enter => self.step(1),
                    FieldOutcome::Ignored if is_text(key) => self.type_elsewhere(key),
                    _ => {}
                }
            }
        }
        None
    }

    /// Sends `key` to the typing field, replacing its text.
    fn type_elsewhere(&mut self, key: &Key) {
        let Some(target) = self.typing_field.clone() else {
            return;
        };
        if target == self.focus || !self.field(&self.focus).is_some_and(Field::passes_typing) {
            return;
        }
        self.typed_from = Some(self.focus.clone());
        self.focus_on(&target);
        if let Some(field) = self.field_mut(&target) {
            field.set_text("");
            field.handle(key);
        }
    }

    /// The focused field's keys, then moving between fields.
    #[must_use]
    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.inserting {
            return vec![("esc", "done"), ("⏎", "next")];
        }
        let mut hints = self
            .field(&self.focus)
            .map(|f| f.hints(self.modal))
            .unwrap_or_default();
        if self.modal {
            if !self.field(&self.focus).is_some_and(Field::takes_text)
                && self.typing_field.is_some()
            {
                hints.push(("i", "name"));
            }
            hints.push(("j k", "move"));
        } else {
            hints.push(("↑↓", "fields"));
        }
        hints
    }

    /// The form as lines `width` cells wide, and the cursor as (line,
    /// column).
    #[must_use]
    /// With `spacious`, a blank row separates fields (when there's room).
    pub fn lines(
        &self,
        width: u16,
        theme: &Theme,
        spacious: bool,
    ) -> (Vec<Line<'static>>, Option<(usize, u16)>) {
        let mut lines = Vec::new();
        let mut cursor = None;
        // One title column for the whole form, so values line up.
        let title_width = self
            .groups
            .iter()
            .flat_map(|g| &g.fields)
            .filter(|f| !self.hidden.iter().any(|k| k == f.key()) && !f.stacked())
            .map(|f| f.title().chars().count())
            .max()
            .unwrap_or(0)
            .min(MAX_TITLE)
            + 2;
        for group in &self.groups {
            let fields: Vec<_> = group
                .fields
                .iter()
                .filter(|f| !self.hidden.iter().any(|k| k == f.key()))
                .collect();
            if fields.is_empty() {
                continue;
            }
            if !lines.is_empty() {
                lines.push(Line::raw(""));
            }
            if !group.title.is_empty() {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(group.title.to_uppercase(), theme.heading),
                ]));
            }
            for (index, field) in fields.into_iter().enumerate() {
                if spacious && index > 0 {
                    lines.push(Line::raw(""));
                }
                self.push_field(
                    field.as_ref(),
                    title_width,
                    width,
                    theme,
                    &mut lines,
                    &mut cursor,
                );
            }
        }
        (lines, cursor)
    }

    /// Draws one field: its row (or title and rows, when stacked), then its
    /// error or, when focused, its description.
    fn push_field(
        &self,
        field: &dyn Field,
        title_width: usize,
        width: u16,
        theme: &Theme,
        lines: &mut Vec<Line<'static>>,
        cursor: &mut Option<(usize, u16)>,
    ) {
        let focused = field.key() == self.focus;
        let bar = if focused {
            Span::styled(format!("{} ", theme.bar), theme.accent)
        } else {
            Span::raw("  ")
        };
        let title: String = field.title().chars().take(MAX_TITLE).collect();
        let title_style = if focused {
            theme.accent.add_modifier(Modifier::BOLD)
        } else {
            theme.muted
        };
        let stacked = field.stacked();
        let prefix = if stacked {
            STACK_INDENT
        } else if title.is_empty() {
            2
        } else {
            2 + title_width
        };
        let room = width.saturating_sub(u16::try_from(prefix).unwrap_or(u16::MAX));
        let focus = match (focused, self.modal, self.inserting) {
            (false, ..) => Focus::Blurred,
            // Without modes, a focused text field is being edited.
            (true, false, _) | (true, true, true) => Focus::Editing,
            (true, true, false) => Focus::Focused,
        };
        let view = field.view(focus, room, theme);
        if stacked {
            lines.push(Line::from(vec![
                bar.clone(),
                Span::styled(field.title().to_owned(), title_style),
            ]));
        }
        let mut spans = vec![bar.clone()];
        if stacked {
            spans.push(Span::raw(" ".repeat(STACK_INDENT - 2)));
        } else if !title.is_empty() {
            spans.push(Span::styled(format!("{title:<title_width$}"), title_style));
        }
        if let Some(column) = view.cursor {
            *cursor = Some((
                lines.len() + view.cursor_line,
                u16::try_from(prefix).unwrap_or(u16::MAX) + column,
            ));
        }
        spans.extend(view.spans);
        lines.push(Line::from(spans));
        for more in view.more {
            let mut spans = vec![bar.clone(), Span::raw(" ".repeat(prefix - 2))];
            spans.extend(more);
            lines.push(Line::from(spans));
        }
        let indent = " ".repeat(prefix);
        if let Some((_, message)) = self.error.as_ref().filter(|(k, _)| k == field.key()) {
            lines.push(Line::from(vec![
                bar.clone(),
                Span::raw(indent[2..].to_owned()),
                Span::styled(format!("✗ {message}"), theme.error),
            ]));
        } else if focused && !field.description().is_empty() {
            lines.push(Line::from(vec![
                bar,
                Span::raw(indent[2..].to_owned()),
                Span::styled(field.description().to_owned(), theme.muted),
            ]));
        }
    }
}

/// A printable character or a paste.
fn is_text(key: &Key) -> bool {
    matches!(key, Key::Char(c) if !c.is_control()) || matches!(key, Key::Paste(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form() -> Form {
        Form::new(vec![
            Group::new(
                "where",
                vec![
                    Box::new(TextInput::new("name", "name").with_description("what to call it")),
                    Box::new(Number::new("count", "count", 0)),
                ],
            ),
            Group::new(
                "",
                vec![
                    Box::new(Select::new("mode", "mode", vec!["a".into(), "b".into()])),
                    Box::new(Button::new("start", "start")),
                ],
            ),
        ])
    }

    fn text(form: &Form) -> String {
        let (lines, _) = form.lines(60, &Theme::default(), false);
        lines
            .iter()
            .map(|l| l.to_string().trim_end().to_owned())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn focus_moves_and_enter_moves_on() {
        let mut form = form();
        assert_eq!(form.focused(), "name");
        form.handle(&Key::Char('x'));
        form.handle(&Key::Enter);
        assert_eq!(form.focused(), "count");
        form.handle(&Key::Down);
        form.handle(&Key::Right);
        assert_eq!(form.value("mode"), Value::Choice(1));
        form.handle(&Key::Down);
        assert_eq!(
            form.handle(&Key::Enter),
            Some(FormEvent::Activated("start".into()))
        );
        assert_eq!(form.value("name"), Value::Text("x".into()));
    }

    #[test]
    fn hidden_fields_are_skipped_and_errors_show_under_their_field() {
        let mut form = form();
        form.set_hidden("count", true);
        form.handle(&Key::Down);
        assert_eq!(form.focused(), "mode");
        form.set_error("name", "give it a name");
        let shown = text(&form);
        assert!(!shown.contains("count"));
        assert!(shown.contains("✗ give it a name"), "{shown}");
        assert!(
            !shown.contains("what to call it"),
            "the error replaces the description"
        );
    }

    #[test]
    fn typing_on_a_button_names_it() {
        let mut form = form().typing_into("name");
        form.handle(&Key::Char('o'));
        form.focus_on("start");
        for c in "auth".chars() {
            form.handle(&Key::Char(c));
        }
        assert_eq!(form.focused(), "name");
        assert_eq!(
            form.value("name"),
            Value::Text("auth".into()),
            "replaced, not appended"
        );
        assert_eq!(
            form.handle(&Key::Enter),
            Some(FormEvent::Activated("start".into())),
            "Enter goes back to the button and presses it"
        );
    }

    #[test]
    fn only_the_focused_field_explains_itself() {
        let mut form = form();
        assert!(text(&form).contains("what to call it"));
        form.handle(&Key::Down);
        assert!(!text(&form).contains("what to call it"));
    }
}
