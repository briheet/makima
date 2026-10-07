//! Agent adapters: what Makima knows about each program it runs as an
//! agent — how to start it, pin and resume its sessions, pass an opening
//! prompt and map [`PermissionMode`]s to its flags.
//!
//! Everything program-specific lives here. Presets pair an adapter with a
//! name, so adding a program means adding one adapter (or configuring a
//! [`Configured`] one), not editing the registry, store or FFI.

use std::fmt;
use std::sync::Arc;

use crate::permission::{PermissionFlags, PermissionMode};
use crate::preset::{Launch, shell_quote};

/// Where a program saves its sessions, so the store can check one exists
/// before resuming it. The core names the place; the store does the I/O.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SessionStore {
    /// `~/.claude/projects/<folder>/<id>.jsonl`, written on the first
    /// message.
    ClaudeCode,
}

/// A program Makima can run as an agent.
///
/// Implementations are cheap, immutable descriptions; presets share them
/// behind an `Arc`.
pub trait AgentAdapter: fmt::Debug + Send + Sync {
    /// The program and its fixed arguments, e.g. `claude`; `None` runs the
    /// user's login shell.
    fn command(&self) -> Option<&str>;

    /// The arguments `launch` adds after [`Self::command`], one word each
    /// and unquoted. Options the program doesn't support are left out.
    fn args(&self, launch: &Launch) -> Vec<String>;

    /// Whether a launch can pin the program's session id.
    fn pins_sessions(&self) -> bool {
        false
    }

    /// Where the program saves sessions; `None` when Makima can't look, in
    /// which case a pinned session is assumed to exist.
    fn sessions(&self) -> Option<SessionStore> {
        None
    }

    /// The flags that start the program in `mode`, if it has any.
    fn permission_flags(&self, mode: PermissionMode) -> Option<&str>;

    /// Whether the program understands any permission mode.
    fn supports_permissions(&self) -> bool {
        PermissionMode::ALL
            .into_iter()
            .any(|mode| self.permission_flags(mode).is_some())
    }

    /// The shell command line for `launch`, or `None` for the login shell.
    fn command_line(&self, launch: &Launch) -> Option<String> {
        let mut line = self.command()?.to_owned();
        for arg in self.args(launch) {
            line.push(' ');
            line.push_str(&shell_quote(&arg));
        }
        Some(line)
    }
}

/// How a program takes Makima's launch options; the arguments shared by
/// every flag-driven adapter.
#[derive(Debug, Clone, Copy)]
struct Flags<'a> {
    session: Option<&'a str>,
    resume: Option<&'a str>,
    prompt: bool,
}

impl Flags<'_> {
    fn args(self, launch: &Launch, permission: Option<&str>) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(id) = &launch.session_id {
            let flag = if launch.resume {
                self.resume
            } else {
                self.session
            };
            if let Some(flag) = flag {
                args.extend([flag.to_owned(), id.clone()]);
            }
        }
        if let Some(flags) = permission {
            args.extend(flags.split_whitespace().map(str::to_owned));
        }
        if let (Some(prompt), true, false) = (&launch.prompt, self.prompt, launch.resume) {
            args.push(prompt.clone());
        }
        args
    }
}

/// The user's login shell.
#[derive(Debug, Clone, Copy, Default)]
pub struct Shell;

impl AgentAdapter for Shell {
    fn command(&self) -> Option<&str> {
        None
    }

    fn args(&self, _launch: &Launch) -> Vec<String> {
        Vec::new()
    }

    fn permission_flags(&self, _mode: PermissionMode) -> Option<&str> {
        None
    }
}

/// Claude Code: pinned and resumable sessions, an opening prompt, and
/// `--permission-mode`.
#[derive(Debug, Clone)]
pub struct ClaudeCode {
    command: String,
}

impl ClaudeCode {
    /// Runs `command`, e.g. `claude` or a wrapper script that execs it.
    #[must_use]
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
        }
    }

    const FLAGS: Flags<'static> = Flags {
        session: Some("--session-id"),
        resume: Some("--resume"),
        prompt: true,
    };
}

impl Default for ClaudeCode {
    fn default() -> Self {
        Self::new("claude")
    }
}

impl AgentAdapter for ClaudeCode {
    fn command(&self) -> Option<&str> {
        Some(&self.command)
    }

    fn args(&self, launch: &Launch) -> Vec<String> {
        Self::FLAGS.args(
            launch,
            launch.permission.and_then(|m| self.permission_flags(m)),
        )
    }

    fn pins_sessions(&self) -> bool {
        true
    }

    fn sessions(&self) -> Option<SessionStore> {
        Some(SessionStore::ClaudeCode)
    }

    fn permission_flags(&self, mode: PermissionMode) -> Option<&str> {
        Some(match mode {
            PermissionMode::Manual => "--permission-mode default",
            PermissionMode::AcceptEdits => "--permission-mode acceptEdits",
            PermissionMode::Plan => "--permission-mode plan",
            PermissionMode::Auto => "--permission-mode auto",
            PermissionMode::Bypass => "--dangerously-skip-permissions",
        })
    }
}

/// Codex: an opening prompt, and its sandbox and approval policy for
/// permissions. Its session ids can't be pinned.
#[derive(Debug, Clone)]
pub struct Codex {
    command: String,
}

impl Codex {
    /// Runs `command`, e.g. `codex`.
    #[must_use]
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
        }
    }

    const FLAGS: Flags<'static> = Flags {
        session: None,
        resume: None,
        prompt: true,
    };
}

impl Default for Codex {
    fn default() -> Self {
        Self::new("codex")
    }
}

impl AgentAdapter for Codex {
    fn command(&self) -> Option<&str> {
        Some(&self.command)
    }

    fn args(&self, launch: &Launch) -> Vec<String> {
        Self::FLAGS.args(
            launch,
            launch.permission.and_then(|m| self.permission_flags(m)),
        )
    }

    fn permission_flags(&self, mode: PermissionMode) -> Option<&str> {
        Some(match mode {
            PermissionMode::Manual => "--sandbox workspace-write --ask-for-approval untrusted",
            PermissionMode::AcceptEdits => {
                "--sandbox workspace-write --ask-for-approval on-request"
            }
            PermissionMode::Plan => "--sandbox read-only --ask-for-approval on-request",
            PermissionMode::Auto => "--sandbox workspace-write --ask-for-approval never",
            PermissionMode::Bypass => "--dangerously-bypass-approvals-and-sandbox",
        })
    }
}

/// A program described entirely by the config's `[[agents]]`: its command,
/// session flags, prompt argument and permission flags.
#[derive(Debug, Clone, Default)]
pub struct Configured {
    command: Option<String>,
    session_flag: Option<String>,
    resume_flag: Option<String>,
    prompt_arg: bool,
    permission_flags: PermissionFlags,
}

impl Configured {
    /// Runs `command` with no options; `None` runs the login shell.
    #[must_use]
    pub fn new(command: Option<String>) -> Self {
        Self {
            command,
            ..Self::default()
        }
    }

    /// Flags that pin and resume the program's session ids.
    #[must_use]
    pub fn with_session_flags(mut self, session: Option<String>, resume: Option<String>) -> Self {
        self.session_flag = session;
        self.resume_flag = resume;
        self
    }

    /// The program takes an opening prompt as its last argument.
    #[must_use]
    pub fn with_prompt_argument(mut self, accepts: bool) -> Self {
        self.prompt_arg = accepts;
        self
    }

    #[must_use]
    pub fn with_permission_flags(mut self, flags: PermissionFlags) -> Self {
        self.permission_flags = flags;
        self
    }
}

impl AgentAdapter for Configured {
    fn command(&self) -> Option<&str> {
        self.command.as_deref()
    }

    fn args(&self, launch: &Launch) -> Vec<String> {
        let flags = Flags {
            session: self.session_flag.as_deref(),
            resume: self.resume_flag.as_deref(),
            prompt: self.prompt_arg,
        };
        flags.args(
            launch,
            launch.permission.and_then(|m| self.permission_flags(m)),
        )
    }

    fn pins_sessions(&self) -> bool {
        self.session_flag.is_some()
    }

    fn permission_flags(&self, mode: PermissionMode) -> Option<&str> {
        self.permission_flags.get(mode)
    }
}

/// The built-in adapter named `name` in the config's `adapter = …`,
/// running `command`, or its usual program when `None`.
///
/// # Errors
/// [`UnknownAdapter`] for a name other than `claude-code` or `codex`.
pub fn builtin_adapter(
    name: &str,
    command: Option<String>,
) -> Result<Arc<dyn AgentAdapter>, UnknownAdapter> {
    match (name, command) {
        ("claude-code", Some(command)) => Ok(Arc::new(ClaudeCode::new(command))),
        ("claude-code", None) => Ok(Arc::new(ClaudeCode::default())),
        ("codex", Some(command)) => Ok(Arc::new(Codex::new(command))),
        ("codex", None) => Ok(Arc::new(Codex::default())),
        (other, _) => Err(UnknownAdapter(other.to_owned())),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown adapter {0:?}; expected claude-code or codex")]
pub struct UnknownAdapter(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_pins_resumes_and_prompts() {
        let claude = ClaudeCode::default();
        let new = Launch::new()
            .with_session("0199-abc")
            .with_prompt(Some("Read brief.md, it's ready".into()));
        assert_eq!(
            claude.command_line(&new).as_deref(),
            Some("claude --session-id 0199-abc 'Read brief.md, it'\\''s ready'")
        );
        let resume = new.resuming(Some("0199-abc".into()));
        assert_eq!(
            claude.command_line(&resume).as_deref(),
            Some("claude --resume 0199-abc")
        );
        assert_eq!(claude.sessions(), Some(SessionStore::ClaudeCode));
    }

    #[test]
    fn permission_modes_become_each_programs_flags() {
        let launch = Launch::new()
            .with_session("s1")
            .with_prompt(Some("go".into()))
            .with_permission(Some(PermissionMode::Bypass));
        assert_eq!(
            ClaudeCode::default().command_line(&launch).as_deref(),
            Some("claude --session-id s1 --dangerously-skip-permissions go")
        );
        let plan = launch.with_permission(Some(PermissionMode::Plan));
        assert_eq!(
            Codex::default().command_line(&plan).as_deref(),
            Some("codex --sandbox read-only --ask-for-approval on-request go")
        );
        assert_eq!(Shell.command_line(&plan), None, "the shell has none");
        assert!(!Shell.supports_permissions());
    }

    #[test]
    fn unsupported_options_are_left_out() {
        let launch = Launch::new()
            .with_session("x")
            .with_prompt(Some("go".into()));
        assert_eq!(
            Codex::default().command_line(&launch).as_deref(),
            Some("codex go")
        );
        assert!(!Codex::default().pins_sessions());
        let plain = Configured::new(Some("aider".into()));
        assert_eq!(plain.command_line(&launch).as_deref(), Some("aider"));
    }

    #[test]
    fn configured_programs_take_their_own_flags() {
        let aider = Configured::new(Some("aider".into()))
            .with_session_flags(Some("--session".into()), None)
            .with_prompt_argument(true)
            .with_permission_flags(PermissionFlags::new([(
                PermissionMode::Bypass,
                "--yes-always".into(),
            )]));
        let launch = Launch::new()
            .with_session("s")
            .with_prompt(Some("go".into()))
            .with_permission(Some(PermissionMode::Bypass));
        assert_eq!(
            aider.command_line(&launch).as_deref(),
            Some("aider --session s --yes-always go")
        );
        assert!(aider.pins_sessions());
        assert_eq!(aider.sessions(), None, "can't be checked, assumed saved");
    }

    #[test]
    fn builtin_adapters_are_named() {
        let claude = builtin_adapter("claude-code", Some("my-claude".into()))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(claude.command(), Some("my-claude"));
        assert_eq!(claude.sessions(), Some(SessionStore::ClaudeCode));
        let codex = builtin_adapter("codex", None).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(codex.command(), Some("codex"));
        assert!(builtin_adapter("gemini", None).is_err());
    }
}
