use std::fmt;
use std::str::FromStr;

/// What kind of work this is, borrowing Conventional Commits' vocabulary.
/// A kind picks a starting template; it never fixes the team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum WorkKind {
    /// Add something new.
    Feat,
    /// Something is broken.
    Fix,
    /// Restructure without changing behaviour.
    Refactor,
    /// Investigate a question; no code changes.
    Research,
    /// Assess a change or some code.
    Review,
    /// Write or update documentation.
    Docs,
}

impl WorkKind {
    pub const ALL: [Self; 6] = [
        Self::Feat,
        Self::Fix,
        Self::Refactor,
        Self::Research,
        Self::Review,
        Self::Docs,
    ];

    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Feat => "feat",
            Self::Fix => "fix",
            Self::Refactor => "refactor",
            Self::Research => "research",
            Self::Review => "review",
            Self::Docs => "docs",
        }
    }

    /// One line explaining the kind, for pickers.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::Feat => "add something new",
            Self::Fix => "something is broken",
            Self::Refactor => "restructure without changing behaviour",
            Self::Research => "investigate a question; no code changes",
            Self::Review => "assess a change or some code",
            Self::Docs => "write or update documentation",
        }
    }
}

impl fmt::Display for WorkKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.id())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown kind {0:?}; expected one of feat, fix, refactor, research, review, docs")]
pub struct UnknownKind(pub String);

impl FromStr for WorkKind {
    type Err = UnknownKind;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|kind| kind.id() == s)
            .ok_or_else(|| UnknownKind(s.to_owned()))
    }
}
