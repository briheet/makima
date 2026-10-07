//! A fuzzy picker, like Helix's buffer picker: type to filter, `↑↓` or
//! `Ctrl-n`/`Ctrl-p` to move, `⏎` to choose, `Esc` to close.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};

use crate::editor::LineEditor;
use crate::fuzzy;
use crate::input::{Key, parse_keys};
use crate::render::{ACCENT, MUTED};

/// One row in a picker.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PickerItem {
    /// Returned when chosen.
    pub id: String,
    pub label: String,
    /// Dimmed after the label, e.g. kind and folder.
    pub detail: String,
}

impl PickerItem {
    #[must_use]
    pub fn new(id: impl Into<String>, label: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            detail: detail.into(),
        }
    }
}

/// What a key did to the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PickerEvent {
    /// The chosen item's id.
    Chosen(String),
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct Picker {
    title: String,
    items: Vec<PickerItem>,
    query: LineEditor,
    selected: usize,
}

const VISIBLE_ROWS: usize = 12;

impl Picker {
    #[must_use]
    pub fn new(title: impl Into<String>, items: Vec<PickerItem>) -> Self {
        Self {
            title: title.into(),
            items,
            query: LineEditor::new(),
            selected: 0,
        }
    }

    /// Applies one chunk of terminal input.
    pub fn feed(&mut self, input: &[u8]) -> Option<PickerEvent> {
        parse_keys(input).iter().find_map(|key| self.handle(key))
    }

    fn handle(&mut self, key: &Key) -> Option<PickerEvent> {
        let matches = self.matches();
        match key {
            Key::Esc | Key::Ctrl('c') => return Some(PickerEvent::Cancelled),
            Key::Enter => {
                return matches
                    .get(self.selected)
                    .map(|&i| PickerEvent::Chosen(self.items[i].id.clone()));
            }
            Key::Up | Key::Ctrl('p') | Key::BackTab => {
                self.selected = self.selected.saturating_sub(1);
            }
            Key::Down | Key::Ctrl('n') | Key::Tab => {
                self.selected = (self.selected + 1).min(matches.len().saturating_sub(1));
            }
            key => {
                self.query.handle(key);
                self.selected = 0;
            }
        }
        None
    }

    /// Indexes of items matching the query, best first.
    fn matches(&self) -> Vec<usize> {
        let labels: Vec<String> = self
            .items
            .iter()
            .map(|i| format!("{} {}", i.label, i.detail))
            .collect();
        fuzzy::filter(&self.query.text(), &labels)
    }

    /// Where the picker's box goes over `body`.
    pub(crate) fn area(&self, body: Rect) -> Rect {
        let rows = self.matches().len().clamp(1, VISIBLE_ROWS);
        let height = u16::try_from(rows + 4).unwrap_or(u16::MAX).min(body.height);
        let width = 72.min(body.width.saturating_sub(4));
        Rect::new(
            body.x + body.width.saturating_sub(width) / 2,
            body.y + body.height.saturating_sub(height) / 3,
            width,
            height,
        )
    }

    /// Draws the picker as a box over `body`; returns the query's cursor.
    pub(crate) fn draw(&self, body: Rect, buffer: &mut Buffer) -> Option<(u16, u16)> {
        let matches = self.matches();
        let area = self.area(body);
        Clear.render(area, buffer);
        let block = Block::bordered()
            .border_style(Style::new().fg(ACCENT))
            .title_top(Line::from(format!(" {} ", self.title)));
        let inner = block.inner(area);
        block.render(area, buffer);
        if inner.height < 2 {
            return None;
        }

        let prompt = Span::styled("› ", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD));
        Paragraph::new(Line::from(vec![prompt, Span::raw(self.query.text())]))
            .render(Rect::new(inner.x, inner.y, inner.width, 1), buffer);
        let cursor = (
            inner.x + 2 + u16::try_from(self.query.cursor()).unwrap_or(0),
            inner.y,
        );

        let start = self.selected.saturating_sub(VISIBLE_ROWS - 1);
        let top = inner.y + 2;
        if matches.is_empty() {
            Paragraph::new(Span::styled("no matches", Style::new().fg(MUTED)))
                .render(Rect::new(inner.x, top, inner.width, 1), buffer);
        }
        let rows = matches.iter().enumerate().skip(start).take(VISIBLE_ROWS);
        for (y, (row, &index)) in (top..).zip(rows) {
            if y >= inner.bottom() {
                break;
            }
            let item = &self.items[index];
            let chosen = row == self.selected;
            let label_style = if chosen {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            };
            Paragraph::new(Line::from(vec![
                Span::styled(if chosen { "▸ " } else { "  " }, Style::new().fg(ACCENT)),
                Span::styled(format!("{:<24}", item.label), label_style),
                Span::styled(item.detail.clone(), Style::new().fg(MUTED)),
            ]))
            .render(Rect::new(inner.x, y, inner.width, 1), buffer);
        }
        Some(cursor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picker() -> Picker {
        Picker::new(
            "work",
            vec![
                PickerItem::new("1", "auth refactor", "feat ~/code/api"),
                PickerItem::new("2", "fix ci", "fix ~/code/makima"),
                PickerItem::new("3", "docs", "docs ~/code/site"),
            ],
        )
    }

    #[test]
    fn filters_moves_and_chooses() {
        let mut picker = picker();
        assert_eq!(picker.feed(b"\x0e"), None);
        assert_eq!(picker.feed(b"\r"), Some(PickerEvent::Chosen("2".into())));

        let mut picker = self::picker();
        picker.feed(b"ci");
        assert_eq!(picker.feed(b"\r"), Some(PickerEvent::Chosen("2".into())));
        assert_eq!(picker.feed(b"\x1b"), Some(PickerEvent::Cancelled));
    }

    #[test]
    fn nothing_matching_chooses_nothing() {
        let mut picker = picker();
        picker.feed(b"zzzz");
        assert_eq!(picker.feed(b"\r"), None);
    }
}
