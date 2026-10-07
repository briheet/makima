//! The brief: what the user wants, assembled from a template's questions
//! and any notes (typed or spoken), written to `brief.md` for the agents.

use std::fmt::Write;

use crate::WorkKind;

/// One note added while writing the brief.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BriefNote {
    pub text: String,
    /// Transcribed from a voice recording.
    pub spoken: bool,
}

impl BriefNote {
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            spoken: false,
        }
    }

    /// A note transcribed from a voice recording.
    #[must_use]
    pub fn spoken(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            spoken: true,
        }
    }
}

/// Everything that goes into `brief.md`. Build with [`Brief::new`] and the
/// `with_*` methods.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Brief<'a> {
    pub title: &'a str,
    pub kind: Option<WorkKind>,
    pub folder: Option<&'a str>,
    pub read_only: bool,
    /// Question and answer pairs; empty answers are left out.
    pub answers: &'a [(String, String)],
    pub notes: &'a [BriefNote],
}

impl<'a> Brief<'a> {
    #[must_use]
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            kind: None,
            folder: None,
            read_only: false,
            answers: &[],
            notes: &[],
        }
    }

    #[must_use]
    pub fn with_kind(mut self, kind: Option<WorkKind>) -> Self {
        self.kind = kind;
        self
    }

    #[must_use]
    pub fn with_folder(mut self, folder: &'a str) -> Self {
        self.folder = Some(folder);
        self
    }

    #[must_use]
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Question and answer pairs; empty answers are left out.
    #[must_use]
    pub fn with_answers(mut self, answers: &'a [(String, String)]) -> Self {
        self.answers = answers;
        self
    }

    #[must_use]
    pub fn with_notes(mut self, notes: &'a [BriefNote]) -> Self {
        self.notes = notes;
        self
    }

    /// The brief as Markdown.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = format!("# {}\n\n", self.title);
        if let Some(kind) = self.kind {
            let _ = writeln!(out, "- **Kind:** {kind} ({})", kind.description());
        }
        if let Some(folder) = self.folder {
            let _ = writeln!(out, "- **Folder:** `{folder}`");
        }
        if self.read_only {
            out.push_str("- **Read-only:** investigate and report; do not modify files.\n");
        }
        for (question, answer) in self.answers.iter().filter(|(_, a)| !a.trim().is_empty()) {
            let _ = write!(out, "\n## {question}\n\n{}\n", answer.trim());
        }
        if !self.notes.is_empty() {
            out.push_str("\n## Notes\n\n");
            for note in self.notes {
                let marker = if note.spoken { " (spoken)" } else { "" };
                let _ = writeln!(out, "- {}{marker}", note.text.trim());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_answers_and_notes() {
        let answers = [
            ("What's broken?".to_owned(), "Login loops".to_owned()),
            ("Expected instead".to_owned(), "   ".to_owned()),
        ];
        let notes = [BriefNote {
            text: "happens on Safari only".into(),
            spoken: true,
        }];
        let brief = Brief {
            title: "fix login",
            kind: Some(WorkKind::Fix),
            folder: Some("~/code/app"),
            read_only: false,
            answers: &answers,
            notes: &notes,
        };
        let text = brief.render();
        assert!(text.starts_with("# fix login\n"));
        assert!(text.contains("## What's broken?\n\nLogin loops"));
        assert!(
            !text.contains("Expected instead"),
            "empty answers are skipped"
        );
        assert!(text.contains("- happens on Safari only (spoken)"));
    }
}
