//! A single-line editor fed raw terminal input bytes, for the talk line,
//! the `makima>` prompt and form fields. It understands the keys a shell
//! line does: arrows, Home/End, Backspace/Delete, ^A ^E ^U ^W ^K, history,
//! and bracketed paste.

use crate::input::{Key, parse_keys};

/// What a chunk of input finished.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditorEvent {
    /// Enter: the line, which the editor then clears.
    Submitted(String),
    /// Escape or ^C.
    Cancelled,
}

#[derive(Debug, Default, Clone)]
pub struct LineEditor {
    text: Vec<char>,
    cursor: usize,
    history: Vec<String>,
    /// Where Up/Down are in `history`; `None` is the line being typed.
    browsing: Option<usize>,
}

impl LineEditor {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn text(&self) -> String {
        self.text.iter().collect()
    }

    /// The cursor, in characters.
    #[must_use]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Replaces the line, cursor at the end.
    pub fn set_text(&mut self, text: &str) {
        self.text = text.chars().collect();
        self.cursor = self.text.len();
        self.browsing = None;
    }

    /// Appends text at the end, with a space if needed (e.g. a voice note).
    pub fn append(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        if self.text.last().is_some_and(|c| !c.is_whitespace()) {
            self.text.push(' ');
        }
        self.text.extend(text.chars());
        self.cursor = self.text.len();
    }

    /// Applies one chunk of terminal input (one key, or a paste).
    pub fn feed(&mut self, input: &[u8]) -> Vec<EditorEvent> {
        parse_keys(input)
            .iter()
            .filter_map(|key| self.handle(key))
            .collect()
    }

    /// Applies one key.
    pub fn handle(&mut self, key: &Key) -> Option<EditorEvent> {
        match key {
            Key::Enter => {
                let line = self.text();
                if !line.trim().is_empty() && self.history.last() != Some(&line) {
                    self.history.push(line.clone());
                }
                self.text.clear();
                self.cursor = 0;
                self.browsing = None;
                return Some(EditorEvent::Submitted(line));
            }
            Key::Esc | Key::Ctrl('c') => return Some(EditorEvent::Cancelled),
            Key::Char(c) => self.insert(*c),
            Key::Paste(text) => text.chars().for_each(|c| self.insert(c)),
            Key::Tab => self.insert(' '),
            Key::Backspace => self.backspace(),
            Key::Delete if self.cursor < self.text.len() => {
                self.text.remove(self.cursor);
            }
            Key::Left | Key::Ctrl('b') => self.cursor = self.cursor.saturating_sub(1),
            Key::Right | Key::Ctrl('f') => self.cursor = (self.cursor + 1).min(self.text.len()),
            Key::Home | Key::Ctrl('a') => self.cursor = 0,
            Key::End | Key::Ctrl('e') => self.cursor = self.text.len(),
            Key::Ctrl('k') => self.text.truncate(self.cursor),
            Key::Ctrl('u') => {
                self.text.drain(..self.cursor);
                self.cursor = 0;
            }
            Key::Ctrl('w') => self.delete_word(),
            Key::Up => self.recall(true),
            Key::Down => self.recall(false),
            _ => {}
        }
        None
    }

    fn insert(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.text.remove(self.cursor);
        }
    }

    fn delete_word(&mut self) {
        let mut start = self.cursor;
        while start > 0 && self.text[start - 1] == ' ' {
            start -= 1;
        }
        while start > 0 && self.text[start - 1] != ' ' {
            start -= 1;
        }
        self.text.drain(start..self.cursor);
        self.cursor = start;
    }

    /// Up (`older`) or Down through history.
    fn recall(&mut self, older: bool) {
        if self.history.is_empty() {
            return;
        }
        let next = match (self.browsing, older) {
            (None, true) => Some(self.history.len() - 1),
            (None, false) => return,
            (Some(0), true) => Some(0),
            (Some(i), true) => Some(i - 1),
            (Some(i), false) if i + 1 < self.history.len() => Some(i + 1),
            (Some(_), false) => None,
        };
        self.browsing = next;
        self.text = next
            .map(|i| self.history[i].chars().collect())
            .unwrap_or_default();
        self.cursor = self.text.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_editing_and_submitting() {
        let mut editor = LineEditor::new();
        assert!(editor.feed(b"hello wrld").is_empty());
        editor.feed(b"\x1b[D\x1b[D\x1b[D");
        editor.feed(b"o");
        assert_eq!(editor.text(), "hello world");
        editor.feed(b"\x17");
        assert_eq!(editor.text(), "hello rld");
        assert_eq!(
            editor.feed(b"\r"),
            vec![EditorEvent::Submitted("hello rld".into())]
        );
        assert_eq!(editor.text(), "");
    }

    #[test]
    fn history_and_cancel() {
        let mut editor = LineEditor::new();
        editor.feed(b"one\r");
        editor.feed(b"two\r");
        editor.feed(b"\x1b[A");
        assert_eq!(editor.text(), "two");
        editor.feed(b"\x1b[A");
        assert_eq!(editor.text(), "one");
        editor.feed(b"\x1b[B\x1b[B");
        assert_eq!(editor.text(), "");
        assert_eq!(editor.feed(b"\x1b"), vec![EditorEvent::Cancelled]);
    }

    #[test]
    fn pasted_newlines_do_not_submit() {
        let mut editor = LineEditor::new();
        assert!(
            editor
                .feed("\x1b[200~fix the\nflaky test\x1b[201~".as_bytes())
                .is_empty()
        );
        assert_eq!(editor.text(), "fix the flaky test");
    }

    #[test]
    fn unicode_is_one_character() {
        let mut editor = LineEditor::new();
        editor.feed("héllo ✓".as_bytes());
        editor.feed(b"\x7f");
        assert_eq!(editor.text(), "héllo ");
        assert_eq!(editor.cursor(), 6);
    }
}
