//! The `makima>` command language.
//!
//! ```text
//! new <name…> [--lead <preset>] [--children <n>] [--child <preset>]
//! add [<preset>] [<n>]      more children for the work on screen
//! help
//! ```

use makima_core::PresetId;

/// A parsed command.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Command {
    /// Start work: a lead plus `children` agents.
    New {
        name: String,
        /// `None` runs the configured default lead.
        lead: Option<PresetId>,
        children: u32,
        /// Preset for the children; defaults to the lead's.
        child: Option<PresetId>,
    },
    /// Add `count` children to the work on screen, running `preset` (the
    /// lead's when `None`).
    Add {
        preset: Option<PresetId>,
        count: u32,
    },
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CommandError {
    #[error("unknown command {0:?}; try: new <name> --lead claude --children 3")]
    Unknown(String),
    #[error("new needs a name: new <name> [--lead claude] [--children 3]")]
    MissingName,
    #[error("{flag} needs a value")]
    MissingValue { flag: String },
    #[error("expected a number of agents, got {0:?}")]
    BadCount(String),
    #[error("add takes at most a preset and a count: add [codex] [3]")]
    TooManyArguments,
    #[error("unknown option {0:?}")]
    UnknownOption(String),
    #[error("unclosed quote")]
    UnclosedQuote,
}

/// Parses one `makima>` line.
///
/// # Errors
/// [`CommandError`] describing what's wrong, with a usage hint.
pub fn parse_command(line: &str) -> Result<Command, CommandError> {
    let words = split_words(line)?;
    let Some((command, args)) = words.split_first() else {
        return Err(CommandError::Unknown(String::new()));
    };
    match command.as_str() {
        "new" | "n" => parse_new(args),
        "add" | "a" => parse_add(args),
        "help" | "?" | "h" => Ok(Command::Help),
        other => Err(CommandError::Unknown(other.to_owned())),
    }
}

fn parse_new(args: &[String]) -> Result<Command, CommandError> {
    let mut name = Vec::new();
    let mut lead = None;
    let mut children = 0;
    let mut child = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| {
            args.next()
                .cloned()
                .ok_or_else(|| CommandError::MissingValue {
                    flag: flag.to_owned(),
                })
        };
        match arg.as_str() {
            "--lead" | "-l" => lead = Some(value(arg)?.into()),
            "--children" | "-c" => {
                let count = value(arg)?;
                children = count.parse().map_err(|_| CommandError::BadCount(count))?;
            }
            "--child" => child = Some(value(arg)?.into()),
            flag if flag.starts_with('-') => {
                return Err(CommandError::UnknownOption(flag.to_owned()));
            }
            word => name.push(word.to_owned()),
        }
    }
    if name.is_empty() {
        return Err(CommandError::MissingName);
    }
    Ok(Command::New {
        name: name.join(" "),
        lead,
        children,
        child,
    })
}

/// `add`, `add 3`, `add codex`, `add codex 3` (either order).
fn parse_add(args: &[String]) -> Result<Command, CommandError> {
    let mut preset = None;
    let mut count = None;
    for arg in args {
        match arg.parse::<u32>() {
            Ok(n) if count.is_none() => count = Some(n),
            Err(_) if preset.is_none() && !arg.starts_with('-') => {
                preset = Some(arg.as_str().into());
            }
            _ => return Err(CommandError::TooManyArguments),
        }
    }
    Ok(Command::Add {
        preset,
        count: count.unwrap_or(1),
    })
}

/// Splits on whitespace, keeping "quoted words" together.
fn split_words(line: &str) -> Result<Vec<String>, CommandError> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut in_word = false;
    for c in line.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => current.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                in_word = true;
            }
            (None, c) if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            (None, c) => {
                current.push(c);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        return Err(CommandError::UnclosedQuote);
    }
    if in_word {
        words.push(current);
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_with_team() {
        assert_eq!(
            parse_command("new auth refactor --lead claude --children 3"),
            Ok(Command::New {
                name: "auth refactor".into(),
                lead: Some("claude".into()),
                children: 3,
                child: None
            })
        );
        assert_eq!(
            parse_command("new \"fix ci\" -l shell --child codex -c 2"),
            Ok(Command::New {
                name: "fix ci".into(),
                lead: Some("shell".into()),
                children: 2,
                child: Some("codex".into())
            })
        );
        assert_eq!(parse_command("  help "), Ok(Command::Help));
        assert!(
            matches!(
                parse_command("new solo"),
                Ok(Command::New { lead: None, .. })
            ),
            "the configured lead"
        );
        assert_eq!(
            parse_command("new big -c 40").map(|c| matches!(c, Command::New { children: 40, .. })),
            Ok(true),
            "no cap on children"
        );
    }

    #[test]
    fn add_children_at_runtime() {
        assert_eq!(
            parse_command("add"),
            Ok(Command::Add {
                preset: None,
                count: 1
            })
        );
        assert_eq!(
            parse_command("add 3 codex"),
            Ok(Command::Add {
                preset: Some("codex".into()),
                count: 3
            })
        );
        assert_eq!(
            parse_command("add codex shell"),
            Err(CommandError::TooManyArguments)
        );
    }

    #[test]
    fn mistakes() {
        assert_eq!(parse_command("new"), Err(CommandError::MissingName));
        assert_eq!(
            parse_command("new x --children lots"),
            Err(CommandError::BadCount("lots".into()))
        );
        assert_eq!(
            parse_command("new x --lead"),
            Err(CommandError::MissingValue {
                flag: "--lead".into()
            })
        );
        assert_eq!(parse_command("new \"x"), Err(CommandError::UnclosedQuote));
        assert!(matches!(
            parse_command("launch"),
            Err(CommandError::Unknown(_))
        ));
    }
}
