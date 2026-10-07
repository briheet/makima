//! Encodes a ratatui buffer as ANSI escape sequences: a full redraw inside a
//! synchronized update (DEC mode 2026), so the terminal never shows half a
//! frame.

use std::fmt::Write;

use ratatui::buffer::{Buffer, CellDiffOption};
use ratatui::style::{Color, Modifier};

pub(crate) fn encode(buffer: &Buffer, cursor: Option<(u16, u16)>) -> String {
    let area = buffer.area;
    let mut out = String::with_capacity(usize::from(area.width) * usize::from(area.height) * 4);
    // Begin synchronized update, hide cursor, no autowrap, home, reset.
    out.push_str("\x1b[?2026h\x1b[?25l\x1b[?7l\x1b[H\x1b[0m");
    for y in area.top()..area.bottom() {
        let _ = write!(out, "\x1b[{};1H", y + 1);
        let mut current = None;
        for x in area.left()..area.right() {
            let cell = &buffer[(x, y)];
            if cell.diff_option == CellDiffOption::Skip {
                continue;
            }
            let style = (cell.fg, cell.bg, cell.modifier);
            if current != Some(style) {
                out.push_str("\x1b[0");
                push_color(&mut out, cell.fg, 38);
                push_color(&mut out, cell.bg, 48);
                push_modifiers(&mut out, cell.modifier);
                out.push('m');
                current = Some(style);
            }
            out.push_str(cell.symbol());
        }
    }
    out.push_str("\x1b[0m");
    if let Some((x, y)) = cursor {
        let _ = write!(out, "\x1b[{};{}H\x1b[?25h", y + 1, x + 1);
    }
    out.push_str("\x1b[?2026l");
    out
}

/// Palette colours are written as indexes so the terminal theme maps them;
/// `Reset` writes nothing, since every style starts from SGR 0.
fn push_color(out: &mut String, color: Color, set: u8) {
    let index = match color {
        Color::Reset => return,
        Color::Black => 0,
        Color::Red => 1,
        Color::Green => 2,
        Color::Yellow => 3,
        Color::Blue => 4,
        Color::Magenta => 5,
        Color::Cyan => 6,
        Color::Gray => 7,
        Color::DarkGray => 8,
        Color::LightRed => 9,
        Color::LightGreen => 10,
        Color::LightYellow => 11,
        Color::LightBlue => 12,
        Color::LightMagenta => 13,
        Color::LightCyan => 14,
        Color::White => 15,
        Color::Indexed(index) => index,
        Color::Rgb(r, g, b) => {
            let _ = write!(out, ";{set};2;{r};{g};{b}");
            return;
        }
    };
    let _ = write!(out, ";{set};5;{index}");
}

fn push_modifiers(out: &mut String, modifier: Modifier) {
    for (flag, code) in [
        (Modifier::BOLD, "1"),
        (Modifier::DIM, "2"),
        (Modifier::ITALIC, "3"),
        (Modifier::UNDERLINED, "4"),
        (Modifier::REVERSED, "7"),
    ] {
        if modifier.contains(flag) {
            out.push(';');
            out.push_str(code);
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;
    use ratatui::style::Style;

    use super::*;

    #[test]
    fn encodes_text_colour_and_cursor() {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 1));
        buffer.set_string(0, 0, "ab", Style::new().fg(Color::Green));
        let ansi = encode(&buffer, Some((2, 0)));
        assert!(ansi.starts_with("\x1b[?2026h"));
        assert!(ansi.contains("\x1b[0;38;5;2mab"));
        assert!(ansi.ends_with("\x1b[1;3H\x1b[?25h\x1b[?2026l"));
    }
}
