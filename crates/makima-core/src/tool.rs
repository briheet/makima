//! Tools: programs opened in a floating window over a work, in its folder
//! (lazygit, a shell, your editor, a file manager). Built-ins can be
//! replaced or added to from the config.

/// A program to open in a floating window.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Tool {
    pub id: String,
    pub label: String,
    /// `None` runs the login shell.
    pub command: Option<String>,
    /// The key after `space` (Helix) or the prefix (tmux); every tool is
    /// also in the tools menu.
    pub key: Option<char>,
    /// The window's share of the screen, 0.3 to 1.
    pub size: f32,
    /// Keep running while hidden (closing only when the program exits).
    pub keep: bool,
}

impl Tool {
    #[must_use]
    pub fn new(id: impl Into<String>, label: impl Into<String>, command: Option<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            command,
            key: None,
            size: Self::DEFAULT_SIZE,
            keep: true,
        }
    }

    pub const DEFAULT_SIZE: f32 = 0.9;

    #[must_use]
    pub fn with_key(mut self, key: Option<char>) -> Self {
        self.key = key;
        self
    }

    #[must_use]
    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    #[must_use]
    pub fn keeping(mut self, keep: bool) -> Self {
        self.keep = keep;
        self
    }

    /// The program it runs (the first word of the command), to check it's
    /// installed; `None` for the shell.
    #[must_use]
    pub fn program(&self) -> Option<&str> {
        self.command.as_deref()?.split_whitespace().next()
    }
}

/// Keys the key maps already use after `space` or the prefix, so a tool
/// can't take them.
pub const RESERVED_TOOL_KEYS: &str = "iao[]:?bcnvxzwThjklO123456789";

/// The tools Makima ships with, in menu order.
#[must_use]
pub fn builtin_tools() -> Vec<Tool> {
    vec![
        Tool::new("git", "lazygit", Some("lazygit".into())).with_key(Some('g')),
        Tool::new("shell", "shell", None).with_key(Some('s')),
        // Through the login shell, so $EDITOR is the user's own.
        Tool::new("editor", "editor", Some("${EDITOR:-vi} .".into())).with_key(Some('e')),
        Tool::new("files", "yazi", Some("yazi".into())).with_key(Some('f')),
    ]
}

/// `base` with `overrides` applied by id, like presets.
#[must_use]
pub fn merge_tools(mut base: Vec<Tool>, overrides: Vec<Tool>) -> Vec<Tool> {
    for tool in overrides {
        match base.iter_mut().find(|t| t.id == tool.id) {
            Some(existing) => *existing = tool,
            None => base.push(tool),
        }
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_have_free_keys_and_overrides_replace_by_id() {
        let tools = builtin_tools();
        for tool in &tools {
            let key = tool.key.unwrap_or('?');
            assert!(!RESERVED_TOOL_KEYS.contains(key), "{}", tool.id);
        }
        let merged = merge_tools(
            tools,
            vec![Tool::new("git", "gitui", Some("gitui".into())).with_key(Some('g'))],
        );
        assert_eq!(merged[0].label, "gitui");
        assert_eq!(merged[0].program(), Some("gitui"));
        assert_eq!(merged[1].program(), None, "the shell");
    }
}
