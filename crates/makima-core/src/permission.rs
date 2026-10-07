//! How much an agent may do without asking: one vocabulary for every
//! program, which each preset turns into its own flags.

use std::fmt;
use std::str::FromStr;

/// An agent's permission mode, from most to least careful. Named after
/// Claude Code's modes; other programs map them to their nearest flags.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum PermissionMode {
    /// Ask before most actions.
    Manual,
    /// Edit files in the work's folder freely; ask before commands.
    AcceptEdits,
    /// Read and plan only; no changes until a plan is approved.
    Plan,
    /// Act freely in the work's folder; risky actions are still checked.
    Auto,
    /// Never ask (Claude's `--dangerously-skip-permissions`). For
    /// sandboxed machines only.
    Bypass,
}

impl PermissionMode {
    pub const ALL: [Self; 5] = [
        Self::Manual,
        Self::AcceptEdits,
        Self::Plan,
        Self::Auto,
        Self::Bypass,
    ];

    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::AcceptEdits => "accept-edits",
            Self::Plan => "plan",
            Self::Auto => "auto",
            Self::Bypass => "bypass",
        }
    }

    /// One line explaining the mode, for pickers.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Self::Manual => "ask before most actions",
            Self::AcceptEdits => "edit freely, ask before commands",
            Self::Plan => "read and plan only",
            Self::Auto => "act freely, risky actions checked",
            Self::Bypass => "never ask (sandboxed machines only)",
        }
    }
}

impl fmt::Display for PermissionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.pad(self.id())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown permission mode {0:?}; expected one of manual, accept-edits, plan, auto, bypass")]
pub struct UnknownPermission(pub String);

impl FromStr for PermissionMode {
    type Err = UnknownPermission;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.id() == s)
            .ok_or_else(|| UnknownPermission(s.to_owned()))
    }
}

/// The flags a program takes for each mode. Modes without flags start the
/// program as configured.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PermissionFlags(Vec<(PermissionMode, String)>);

impl PermissionFlags {
    #[must_use]
    pub fn new(flags: impl IntoIterator<Item = (PermissionMode, String)>) -> Self {
        Self(flags.into_iter().collect())
    }

    /// The flags for `mode`, if the program has any.
    #[must_use]
    pub fn get(&self, mode: PermissionMode) -> Option<&str> {
        self.0
            .iter()
            .find(|(m, _)| *m == mode)
            .map(|(_, flags)| flags.as_str())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for mode in PermissionMode::ALL {
            assert_eq!(mode.id().parse::<PermissionMode>(), Ok(mode));
        }
        assert!("yolo".parse::<PermissionMode>().is_err());
    }
}
