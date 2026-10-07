//! Raw terminal input bytes → keys. Ghostty encodes one key (or one paste)
//! per chunk, the way a TUI reads them from stdin.

/// One key press, or a bracketed paste.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Key {
    Char(char),
    Enter,
    Esc,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Tab,
    BackTab,
    Backspace,
    Delete,
    /// Ctrl plus a letter, lowercase.
    Ctrl(char),
    /// Bracketed paste; newlines become spaces.
    Paste(String),
}

/// Splits one chunk of input into keys.
#[must_use]
pub fn parse_keys(input: &[u8]) -> Vec<Key> {
    let text = String::from_utf8_lossy(input);
    let mut chars = text.chars().peekable();
    let mut keys = Vec::new();
    while let Some(c) = chars.next() {
        let key = match c {
            '\x1b' => match chars.peek() {
                Some('[' | 'O') => {
                    chars.next();
                    let mut sequence = String::new();
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        sequence.push(next);
                        if next.is_ascii_alphabetic() || next == '~' {
                            break;
                        }
                    }
                    if sequence == "200~" {
                        // Collect everything up to the paste's end marker.
                        let mut pasted = String::new();
                        let rest: String = chars.by_ref().collect();
                        let (body, _) = rest.split_once("\x1b[201~").unwrap_or((&rest, ""));
                        pasted.push_str(body);
                        keys.push(Key::Paste(pasted.replace(['\r', '\n'], " ")));
                        break;
                    }
                    match sequence.as_str() {
                        "A" => Key::Up,
                        "B" => Key::Down,
                        "C" => Key::Right,
                        "D" => Key::Left,
                        "H" | "1~" => Key::Home,
                        "F" | "4~" => Key::End,
                        "3~" => Key::Delete,
                        "Z" => Key::BackTab,
                        _ => continue,
                    }
                }
                _ => Key::Esc,
            },
            '\r' | '\n' => Key::Enter,
            '\t' => Key::Tab,
            '\x7f' | '\x08' => Key::Backspace,
            c if (c as u32) < 0x20 => Key::Ctrl(char::from(b'a' + (c as u8) - 1)),
            c => Key::Char(c),
        };
        keys.push(key);
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys() {
        assert_eq!(parse_keys(b"a\r"), vec![Key::Char('a'), Key::Enter]);
        assert_eq!(
            parse_keys(b"\x1b[A\x1b[Z\x1b"),
            vec![Key::Up, Key::BackTab, Key::Esc]
        );
        assert_eq!(parse_keys(b"\x15"), vec![Key::Ctrl('u')]);
        assert_eq!(
            parse_keys("\x1b[200~two\nlines\x1b[201~".as_bytes()),
            vec![Key::Paste("two lines".into())]
        );
    }
}
