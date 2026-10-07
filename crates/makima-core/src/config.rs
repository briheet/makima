//! The user's configuration, read from `config.toml`.
//!
//! Parsing is strict: unknown keys are errors and numbers are range-checked,
//! so a typo is reported instead of silently ignored. Reading the file is
//! left to callers; this module only parses and validates text.

use serde::{Deserialize, Serialize};

use crate::adapter::{AgentAdapter, Configured, builtin_adapter};
use crate::color::Rgb;
use crate::preset::{AgentPreset, DEFAULT_LEAD, PresetId, builtin_presets, merge_presets};
use crate::template::{BriefQuestion, TeamMember, Template, builtin_templates, merge_templates};
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::permission::{PermissionFlags, PermissionMode};
use crate::tool::{RESERVED_TOOL_KEYS, Tool, builtin_tools, merge_tools};
use crate::{AgentRole, WorkKind};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Config {
    pub appearance: Appearance,
    pub terminal: Terminal,
    pub keys: Keys,
    pub storage: Storage,
    pub work: WorkSettings,
    pub voice: Voice,
    pub permissions: Permissions,
    /// Extra presets, or replacements for built-in ones with the same id.
    pub agents: Vec<PresetConfig>,
    /// Extra templates, or replacements for built-in ones with the same id.
    pub templates: Vec<TemplateConfig>,
    /// Extra tools, or replacements for built-in ones with the same id.
    pub tools: Vec<ToolConfig>,
}

/// A tool from the config: a program opened in a floating window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[non_exhaustive]
pub struct ToolConfig {
    pub id: String,
    pub label: String,
    /// `None` runs the login shell.
    pub command: Option<String>,
    /// One character, after `space` (Helix) or the prefix (tmux).
    pub key: Option<String>,
    /// Share of the screen, 0.3 to 1.
    #[serde(default = "default_tool_size")]
    pub size: f32,
    /// Keep running while hidden.
    #[serde(default = "default_true")]
    pub keep: bool,
}

fn default_tool_size() -> f32 {
    Tool::DEFAULT_SIZE
}

fn default_true() -> bool {
    true
}

/// Key bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Keys {
    pub style: KeyStyle,
    /// The key that switches from the agent to Makima's keys.
    pub leader: Leader,
    /// Moving between panes while typing in one, without Ctrl+Space.
    pub navigate: Navigate,
}

/// The key that switches from typing into an agent to Makima's keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Leader {
    /// ⌘Esc. macOS doesn't deliver it to apps as a key press, so it may
    /// not work.
    CmdEsc,
    /// Ctrl+Space: no agent, shell or lazygit uses it.
    #[default]
    CtrlSpace,
    /// Ctrl+O; Claude Code uses it to show its full transcript.
    CtrlO,
    #[serde(rename = "ctrl-]")]
    CtrlBracket,
    ShiftEsc,
}

/// The modifier that, with `h j k l`, moves between panes while typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Navigate {
    /// Ctrl+h/j/k/l, like vim-tmux-navigator. Takes those keys from the
    /// agents (Ctrl+h backspace, Ctrl+l clear in shells).
    #[default]
    Ctrl,
    /// Alt (Option)+h/j/k/l, like Zellij.
    Alt,
    /// Only through Makima's keys (Ctrl+Space, then h/j/k/l).
    Off,
}

/// How Makima's own keys work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum KeyStyle {
    /// Modal, like Helix: `Ctrl+Space` switches to NOR, where `h j k l`,
    /// `g`, `Space`, `Ctrl-w` and `:` are commands, until `i`.
    #[default]
    Helix,
    /// A prefix, like tmux: `Ctrl+Space` then one key.
    Tmux,
}

/// How much agents may do without asking.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Permissions {
    /// For every agent; `None` leaves each program to its own default.
    pub default: Option<PermissionMode>,
    /// Per preset id, replacing `default`.
    pub agents: BTreeMap<PresetId, PermissionMode>,
}

impl Permissions {
    /// The mode for agents started from `preset_id`.
    #[must_use]
    pub fn mode_for(&self, preset_id: &PresetId) -> Option<PermissionMode> {
        self.agents.get(preset_id).copied().or(self.default)
    }
}

/// Where Makima keeps work: one folder (and `SQLite` database) per work.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Storage {
    /// Overrides `~/.local/share/makima`, e.g. to keep work in a vault.
    /// `~` is expanded.
    pub root: Option<String>,
}

/// Defaults for starting work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[non_exhaustive]
pub struct WorkSettings {
    /// Folders searched for git repos to offer in the folder picker.
    /// Missing ones are skipped; `~` is expanded.
    pub roots: Vec<String>,
    /// How many folders down to look (1 to 10).
    pub depth: usize,
    /// The preset new work's lead runs, unless a template names another.
    pub lead: PresetId,
}

impl Default for WorkSettings {
    fn default() -> Self {
        Self {
            roots: vec!["~".to_owned()],
            depth: 5,
            lead: DEFAULT_LEAD.into(),
        }
    }
}

/// Speech to text for voice notes and talking to agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Voice {
    pub provider: VoiceProvider,
    /// A BCP 47 language tag such as `en-US`; the system language if unset.
    pub language: Option<String>,
    /// The transcription model, for `openai`.
    pub model: String,
    /// The base URL of an OpenAI-compatible API (`OpenAI`, Groq, a local
    /// Whisper server).
    pub base_url: String,
    /// The environment variable holding the API key. The key is never
    /// written in the config; without the variable, the macOS Keychain item
    /// `makima` / `<provider>` is used.
    pub api_key_env: String,
}

impl Default for Voice {
    fn default() -> Self {
        Self {
            provider: VoiceProvider::default(),
            language: None,
            model: "gpt-4o-transcribe".into(),
            base_url: "https://api.openai.com/v1".into(),
            api_key_env: "OPENAI_API_KEY".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum VoiceProvider {
    /// On-device: macOS 26's `SpeechAnalyzer`. Private and free.
    #[default]
    Apple,
    /// An OpenAI-compatible `/audio/transcriptions` API.
    Openai,
    /// Voice input turned off.
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Appearance {
    pub mode: AppearanceMode,
    pub theme: ThemeSetting,
    /// Replaces the accent colour picked from the theme.
    pub accent: Option<Rgb>,
    /// 0 (softest) to 1 (strongest).
    pub contrast: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            mode: AppearanceMode::System,
            theme: ThemeSetting::default(),
            accent: None,
            contrast: 0.5,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum AppearanceMode {
    /// Follow macOS light/dark mode.
    #[default]
    System,
    Light,
    Dark,
}

/// One theme for both modes, or a light/dark pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum ThemeSetting {
    Single(String),
    Pair { light: String, dark: String },
}

impl ThemeSetting {
    pub const DEFAULT_LIGHT: &str = "GitHub Light Default";
    pub const DEFAULT_DARK: &str = "Vesper";

    #[must_use]
    pub fn light(&self) -> &str {
        match self {
            Self::Single(name) => name,
            Self::Pair { light, .. } => light,
        }
    }

    #[must_use]
    pub fn dark(&self) -> &str {
        match self {
            Self::Single(name) => name,
            Self::Pair { dark, .. } => dark,
        }
    }
}

impl Default for ThemeSetting {
    fn default() -> Self {
        Self::Pair {
            light: Self::DEFAULT_LIGHT.to_owned(),
            dark: Self::DEFAULT_DARK.to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
#[non_exhaustive]
pub struct Terminal {
    /// `None` uses Ghostty's default font.
    pub font_family: Option<String>,
    pub font_size: f32,
    pub padding_x: u16,
    pub padding_y: u16,
    /// How much to dim terminals that don't have focus, 0 (off) to 0.6.
    pub unfocused_dim: f32,
    /// The cursor's shape, where the program doesn't set its own.
    pub cursor_style: CursorStyle,
    /// Whether the cursor blinks.
    pub cursor_blink: bool,
    /// The shell for shell panes and the shell tool, e.g. `nu`. Unset:
    /// Ghostty's `command`, else the login shell.
    pub shell: Option<String>,
}

/// A terminal cursor's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum CursorStyle {
    #[default]
    Block,
    Bar,
    Underline,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            font_family: None,
            font_size: 14.0,
            padding_x: 0,
            padding_y: 0,
            unfocused_dim: 0.2,
            cursor_style: CursorStyle::Block,
            cursor_blink: false,
            shell: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[non_exhaustive]
pub struct PresetConfig {
    pub id: PresetId,
    pub label: String,
    /// A built-in adapter (`claude-code` or `codex`) that knows the
    /// program's sessions and flags; the other fields describe it otherwise.
    pub adapter: Option<String>,
    /// `None` runs the login shell.
    pub command: Option<String>,
    /// Flag that pins the program's session id, e.g. `--session-id`.
    pub session_flag: Option<String>,
    /// Flag that resumes a session by id, e.g. `--resume`.
    pub resume_flag: Option<String>,
    /// The program takes an opening prompt as its last argument.
    #[serde(default)]
    pub prompt_arg: bool,
    /// The program's flags for each permission mode, e.g.
    /// `{ bypass = "--yes-always" }`, split on whitespace. Modes left out
    /// add no flags.
    #[serde(default)]
    pub permission_flags: BTreeMap<PermissionMode, String>,
}

impl PresetConfig {
    /// The adapter this entry describes: a built-in one running `command`,
    /// or one configured from the flags.
    ///
    /// # Errors
    /// [`UnknownAdapter`](crate::UnknownAdapter) for an `adapter` name
    /// Makima doesn't have.
    pub fn adapter(&self) -> Result<Arc<dyn AgentAdapter>, crate::UnknownAdapter> {
        if let Some(name) = &self.adapter {
            return builtin_adapter(name, self.command.clone());
        }
        Ok(Arc::new(
            Configured::new(self.command.clone())
                .with_session_flags(self.session_flag.clone(), self.resume_flag.clone())
                .with_prompt_argument(self.prompt_arg)
                .with_permission_flags(PermissionFlags::new(
                    self.permission_flags.iter().map(|(m, f)| (*m, f.clone())),
                )),
        ))
    }
}

/// A template from the config: a team shape and the brief's questions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
#[non_exhaustive]
pub struct TemplateConfig {
    pub id: String,
    pub label: String,
    pub kind: Option<WorkKind>,
    #[serde(default)]
    pub description: String,
    /// The lead's preset; `[work] lead` when unset.
    pub lead: Option<PresetId>,
    /// How many children to start with; more can be added while it runs.
    #[serde(default)]
    pub children: u32,
    /// The children's preset; the lead's when unset.
    pub child: Option<PresetId>,
    /// One role per child (planner, implementer, reviewer, tester,
    /// researcher, or any word); "implementer" when there are fewer.
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub read_only: bool,
    /// The team's permission mode; read-only templates default to `plan`,
    /// others to `[permissions]`.
    pub permission: Option<PermissionMode>,
    /// The brief's questions.
    #[serde(default)]
    pub questions: Vec<String>,
    /// What the lead is told at start; placeholders are listed in
    /// `makima_core::prompt`. The general prompt when unset.
    pub lead_prompt: Option<String>,
    /// What each child is told at start.
    pub child_prompt: Option<String>,
}

impl TemplateConfig {
    fn to_template(&self, default_lead: &PresetId) -> Template {
        let role = |i: usize| {
            self.roles.get(i).map_or(AgentRole::Implementer, |word| {
                AgentRole::from(word.as_str())
            })
        };
        let lead = self.lead.as_ref().unwrap_or(default_lead).clone();
        let mut template = Template::new(&self.id, &self.label, lead);
        template.kind = self.kind;
        template.description.clone_from(&self.description);
        template.children = (0..usize::try_from(self.children).unwrap_or(usize::MAX))
            .map(|i| TeamMember {
                preset: self.child.clone(),
                role: role(i),
            })
            .collect();
        template.read_only = self.read_only;
        if let Some(prompt) = &self.lead_prompt {
            template.lead_prompt.clone_from(prompt);
        }
        if let Some(prompt) = &self.child_prompt {
            template.child_prompt.clone_from(prompt);
        }
        template.permission = self
            .permission
            .or(self.read_only.then_some(PermissionMode::Plan));
        template.questions = self
            .questions
            .iter()
            .map(|q| BriefQuestion {
                label: q.clone(),
                hint: String::new(),
            })
            .collect();
        template
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
    #[error(transparent)]
    Parse(#[from] toml::de::Error),
    #[error("{0}")]
    Invalid(String),
}

/// A commented config with every default spelled out, for `config init`.
pub const DEFAULT_CONFIG: &str = r##"# Makima configuration. Every value below is the default.
# Theme names come from the Ghostty theme catalog (iTerm2-Color-Schemes).

[appearance]
mode = "system"                 # system | light | dark
theme = { light = "GitHub Light Default", dark = "Vesper" }   # or: theme = "Rose Pine"
# accent = "#7aa2f7"            # default: the theme's cursor colour, else ANSI blue
contrast = 0.5                  # 0 = softest, 1 = strongest

[terminal]
# font-family = "JetBrains Mono"
font-size = 14                  # ⌘+ ⌘− ⌘0 zoom while running
padding-x = 0                   # 0 keeps text flush with the pane borders, like tmux
padding-y = 0
unfocused-dim = 0.2             # dim unfocused terminals; 0 turns it off
cursor-style = "block"          # block | bar | underline (programs may set their own)
cursor-blink = false            # true makes it blink
# shell = "nu"                  # shell panes and the shell tool; default: Ghostty's
                                # `command`, else your login shell

[keys]
style = "helix"                 # helix (modal: leader for NOR, i to type) | tmux (leader, then one key)
leader = "ctrl-space"           # ctrl-space | shift-esc | ctrl-] | ctrl-o
navigate = "ctrl"               # ctrl | alt | off: Ctrl+h/j/k/l moves between panes while typing

[storage]
# root = "~/notes/makima"       # default: ~/.local/share/makima (one folder per work)

[work]
roots = ["~"]                   # where to look for repos (Library, media folders,
                                # dependencies and hidden folders are skipped)
depth = 5                       # how many folders down
lead = "claude"                 # the preset new work's lead runs

[permissions]
# How much agents may do without asking. Unset leaves each program to its
# own default. manual | accept-edits | plan | auto | bypass
# (bypass is Claude's --dangerously-skip-permissions: sandboxed machines only)
# default = "auto"
# agents = { codex = "accept-edits" }   # per preset, replacing the default

[voice]
provider = "apple"              # apple (on-device, macOS 26) | openai | none
# language = "en-US"            # default: the system language
# For provider = "openai" (any OpenAI-compatible API):
model = "gpt-4o-transcribe"     # or e.g. "whisper-large-v3" on Groq
base-url = "https://api.openai.com/v1"   # Groq: "https://api.groq.com/openai/v1"
api-key-env = "OPENAI_API_KEY"  # the key's environment variable; never put the
                                # key here. Apps opened from the Dock don't see
                                # your shell's variables, so Makima also reads the
                                # Keychain: security add-generic-password -s makima -a openai -w

# Add agent presets, or replace a built-in one (shell, claude, codex) by id.
# A built-in adapter knows its program's sessions and flags:
# [[agents]]
# id = "claude-work"
# label = "Claude (work account)"
# adapter = "claude-code"       # claude-code | codex
# command = "claude-work"       # default: the adapter's usual program
#
# Any other program is described by its flags:
# [[agents]]
# id = "aider"
# label = "Aider"
# command = "aider"
# session-flag = "--session-id" # lets Makima link and resume its sessions
# resume-flag = "--resume"
# prompt-arg = true             # takes the opening prompt as an argument
# permission-flags = { auto = "--yes", bypass = "--yes-always" }

# Tools open in a floating window over a work, in its folder. After
# Ctrl+Space: g (git), s (shell), e (editor), f (files), o for all of them
# (tmux style: ^Space g … and ^Space T). Add your own, or replace a
# built-in one (git, shell, editor, files) by id:
# [[tools]]
# id = "tests"
# label = "watch tests"
# command = "cargo watch -x test"
# key = "t"                     # optional; one free character
# size = 0.9                    # share of the screen, 0.3 to 1
# keep = true                   # keep running while hidden

# Add templates, or replace a built-in one (feat, fix, refactor, research,
# review, docs) by id:
# [[templates]]
# id = "review-team"
# label = "Review team"
# kind = "review"
# lead = "claude"               # default: [work] lead
# children = 2
# child = "codex"
# roles = ["reviewer", "reviewer"]
# read-only = true              # agents plan only (permission = "plan")
# permission = "auto"           # the team's mode; default: [permissions]
# What each agent is told at start. Placeholders: {name} {kind} {folder}
# {brief} {plan} {team} {protocol}, and for children {number} {role} {duty}.
# lead-prompt = "You lead \"{name}\" in {folder}. Read {brief}, split it between {team}, and start."
# child-prompt = "You are the {role}. {duty} Read {brief}, then wait for the lead."
# questions = ["What to review", "Focus on"]
"##;

impl Config {
    /// Parses and validates a TOML config.
    ///
    /// # Errors
    /// [`ConfigError::Parse`] with the location of a syntax error or unknown
    /// key, or [`ConfigError::Invalid`] for an out-of-range value.
    pub fn from_toml(text: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(text)?;
        config.validate()?;
        Ok(config)
    }

    /// Built-in presets with the config's `[[agents]]` applied.
    #[must_use]
    pub fn presets(&self) -> Vec<AgentPreset> {
        let overrides = self
            .agents
            .iter()
            .filter_map(|p| Some(AgentPreset::new(p.id.clone(), &p.label, p.adapter().ok()?)))
            .collect();
        merge_presets(builtin_presets(), overrides)
    }

    /// Built-in tools with the config's `[[tools]]` applied.
    #[must_use]
    pub fn tools(&self) -> Vec<Tool> {
        merge_tools(
            builtin_tools(),
            self.tools
                .iter()
                .map(|t| {
                    Tool::new(&t.id, &t.label, t.command.clone())
                        .with_key(t.key.as_deref().and_then(|k| k.chars().next()))
                        .with_size(t.size)
                        .keeping(t.keep)
                })
                .collect(),
        )
    }

    /// Built-in templates with the config's `[[templates]]` applied.
    #[must_use]
    pub fn templates(&self) -> Vec<Template> {
        let lead = &self.work.lead;
        let builtin = builtin_templates().into_iter().map(|mut t| {
            t.lead.clone_from(lead);
            t
        });
        merge_templates(
            builtin.collect(),
            self.templates.iter().map(|t| t.to_template(lead)).collect(),
        )
    }

    /// `[[agents]]`: ids, commands, and adapters with no extra flags.
    fn validate_agents(&self) -> Result<(), ConfigError> {
        let invalid = |message: String| Err(ConfigError::Invalid(message));
        let mut seen = std::collections::HashSet::new();
        for preset in &self.agents {
            if preset.id.as_str().trim().is_empty() || preset.label.trim().is_empty() {
                return invalid("every [[agents]] entry needs an id and a label".into());
            }
            if !seen.insert(&preset.id) {
                return invalid(format!("[[agents]] id {:?} appears twice", preset.id));
            }
            if preset
                .command
                .as_deref()
                .is_some_and(|c| c.trim().is_empty())
            {
                return invalid(format!("[[agents]] {:?} has an empty command", preset.id));
            }
            if let Err(error) = preset.adapter() {
                return invalid(format!("[[agents]] {:?}: {error}", preset.id));
            }
            let has_flags = preset.session_flag.is_some()
                || preset.resume_flag.is_some()
                || preset.prompt_arg
                || !preset.permission_flags.is_empty();
            if preset.adapter.is_some() && has_flags {
                return invalid(format!(
                    "[[agents]] {:?} names an adapter, which knows its own flags; \
                     drop session-flag, resume-flag, prompt-arg and permission-flags",
                    preset.id
                ));
            }
        }
        Ok(())
    }

    /// `[[tools]]`: ids, sizes, and keys that are free and unique.
    fn validate_tools(&self) -> Result<(), ConfigError> {
        let invalid = |message: String| Err(ConfigError::Invalid(message));
        let mut tool_ids = std::collections::HashSet::new();
        for tool in &self.tools {
            if tool.id.trim().is_empty() || tool.label.trim().is_empty() {
                return invalid("every [[tools]] entry needs an id and a label".into());
            }
            if !tool_ids.insert(&tool.id) {
                return invalid(format!("[[tools]] id {:?} appears twice", tool.id));
            }
            if !(0.3..=1.0).contains(&tool.size) {
                return invalid(format!(
                    "[[tools]] {:?}: size must be 0.3 to 1, got {}",
                    tool.id, tool.size
                ));
            }
            if let Some(key) = &tool.key {
                let mut chars = key.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) if !RESERVED_TOOL_KEYS.contains(c) && !c.is_whitespace() => {}
                    (Some(c), None) => {
                        return invalid(format!(
                            "[[tools]] {:?}: key {c:?} is taken by Makima's own keys",
                            tool.id
                        ));
                    }
                    _ => {
                        return invalid(format!(
                            "[[tools]] {:?}: key must be one character, got {key:?}",
                            tool.id
                        ));
                    }
                }
            }
        }
        let tools = self.tools();
        for (i, tool) in tools.iter().enumerate() {
            if let Some(key) = tool.key
                && let Some(other) = tools[..i].iter().find(|t| t.key == Some(key))
            {
                return invalid(format!(
                    "tools {:?} and {:?} both use key {key:?}",
                    other.id, tool.id
                ));
            }
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), ConfigError> {
        let invalid = |message: String| Err(ConfigError::Invalid(message));
        let a = &self.appearance;
        let t = &self.terminal;
        if !(0.0..=1.0).contains(&a.contrast) {
            return invalid(format!(
                "appearance.contrast must be 0 to 1, got {}",
                a.contrast
            ));
        }
        if a.theme.light().trim().is_empty() || a.theme.dark().trim().is_empty() {
            return invalid("appearance.theme names must not be empty".into());
        }
        if !(6.0..=72.0).contains(&t.font_size) {
            return invalid(format!(
                "terminal.font-size must be 6 to 72, got {}",
                t.font_size
            ));
        }
        if t.padding_x > 64 || t.padding_y > 64 {
            return invalid("terminal.padding-x and padding-y must be at most 64".into());
        }
        if !(0.0..=0.6).contains(&t.unfocused_dim) {
            return invalid(format!(
                "terminal.unfocused-dim must be 0 to 0.6, got {}",
                t.unfocused_dim
            ));
        }
        if !(1..=10).contains(&self.work.depth) {
            return invalid(format!(
                "work.depth must be 1 to 10, got {}",
                self.work.depth
            ));
        }
        let v = &self.voice;
        if v.provider == VoiceProvider::Openai
            && (v.model.trim().is_empty()
                || v.api_key_env.trim().is_empty()
                || !(v.base_url.starts_with("https://") || v.base_url.starts_with("http://")))
        {
            return invalid(
                "voice: provider = \"openai\" needs a model, an api-key-env and an http(s) base-url"
                    .into(),
            );
        }
        self.validate_agents()?;
        self.validate_tools()?;
        let presets = self.presets();
        if !presets.iter().any(|p| *p.id() == self.work.lead) {
            return invalid(format!(
                "work.lead names unknown preset {:?}",
                self.work.lead.as_str()
            ));
        }
        for id in self.permissions.agents.keys() {
            if !presets.iter().any(|p| p.id() == id) {
                return invalid(format!(
                    "[permissions.agents] names unknown preset {:?}",
                    id.as_str()
                ));
            }
        }
        let mut template_ids = std::collections::HashSet::new();
        for template in &self.templates {
            if template.id.trim().is_empty() || template.label.trim().is_empty() {
                return invalid("every [[templates]] entry needs an id and a label".into());
            }
            if !template_ids.insert(&template.id) {
                return invalid(format!("[[templates]] id {:?} appears twice", template.id));
            }
            for preset in template.lead.iter().chain(&template.child) {
                if !presets.iter().any(|p| p.id() == preset) {
                    return invalid(format!(
                        "[[templates]] {:?} uses unknown preset {:?}",
                        template.id,
                        preset.as_str()
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use super::*;

    #[test]
    fn the_template_is_the_default() {
        assert_eq!(
            Config::from_toml(DEFAULT_CONFIG).unwrap(),
            Config::default()
        );
        assert_eq!(Config::from_toml("").unwrap(), Config::default());
    }

    #[test]
    fn themes_accept_a_name_or_a_pair() {
        let single = Config::from_toml("[appearance]\ntheme = \"Rose Pine\"").unwrap();
        assert_eq!(single.appearance.theme.light(), "Rose Pine");
        assert_eq!(single.appearance.theme.dark(), "Rose Pine");
        let pair =
            Config::from_toml("[appearance]\ntheme = { light = \"A\", dark = \"B\" }").unwrap();
        assert_eq!(
            (pair.appearance.theme.light(), pair.appearance.theme.dark()),
            ("A", "B")
        );
    }

    #[test]
    fn permissions_resolve_per_agent_and_template() {
        let config = Config::from_toml(
            "[permissions]\ndefault = \"auto\"\nagents = { codex = \"accept-edits\" }\n\
             [[agents]]\nid = \"aider\"\nlabel = \"Aider\"\ncommand = \"aider\"\n\
             permission-flags = { bypass = \"--yes-always\" }\n\
             [[templates]]\nid = \"audit\"\nlabel = \"Audit\"\nread-only = true\n",
        )
        .unwrap();
        assert_eq!(
            config.permissions.mode_for(&"claude".into()),
            Some(PermissionMode::Auto)
        );
        assert_eq!(
            config.permissions.mode_for(&"codex".into()),
            Some(PermissionMode::AcceptEdits)
        );
        let aider = config
            .presets()
            .into_iter()
            .find(|p| p.id() == "aider")
            .unwrap();
        assert_eq!(
            aider.adapter().permission_flags(PermissionMode::Bypass),
            Some("--yes-always")
        );
        let audit = config
            .templates()
            .into_iter()
            .find(|t| t.id == "audit")
            .unwrap();
        assert_eq!(audit.permission, Some(PermissionMode::Plan));

        let unknown = Config::from_toml("[permissions]\nagents = { gpt = \"auto\" }").unwrap_err();
        assert!(unknown.to_string().contains("gpt"), "{unknown}");
        let typo = Config::from_toml("[permissions]\ndefault = \"yolo\"").unwrap_err();
        assert!(typo.to_string().contains("yolo"), "{typo}");
    }

    #[test]
    fn voice_can_use_an_api() {
        let config = Config::from_toml(
            "[voice]\nprovider = \"openai\"\nmodel = \"whisper-large-v3\"\nbase-url = \"https://api.groq.com/openai/v1\"\napi-key-env = \"GROQ_API_KEY\"",
        )
        .unwrap();
        assert_eq!(config.voice.provider, VoiceProvider::Openai);
        assert_eq!(config.voice.model, "whisper-large-v3");
        let bad = Config::from_toml("[voice]\nprovider = \"openai\"\nbase-url = \"ftp://x\"")
            .unwrap_err();
        assert!(bad.to_string().contains("base-url"), "{bad}");
        let key = Config::from_toml("[voice]\napi-key = \"sk-123\"").unwrap_err();
        assert!(
            key.to_string().contains("api-key"),
            "keys don't go in the config: {key}"
        );
    }

    #[test]
    fn tools_can_be_added_and_keys_are_checked() {
        let config = Config::from_toml(
            "[[tools]]\nid = \"tests\"\nlabel = \"watch tests\"\ncommand = \"cargo watch -x test\"\nkey = \"d\"\nsize = 0.6",
        )
        .unwrap();
        let tools = config.tools();
        assert_eq!(tools.last().unwrap().key, Some('d'));
        assert!((tools.last().unwrap().size - 0.6).abs() < f32::EPSILON);
        let taken =
            Config::from_toml("[[tools]]\nid = \"x\"\nlabel = \"x\"\nkey = \"b\"").unwrap_err();
        assert!(taken.to_string().contains("taken"), "{taken}");
        let twice =
            Config::from_toml("[[tools]]\nid = \"x\"\nlabel = \"x\"\nkey = \"g\"").unwrap_err();
        assert!(twice.to_string().contains("both use key"), "{twice}");
    }

    #[test]
    fn the_example_config_is_valid() {
        let config = Config::from_toml(include_str!("../../../example.toml")).unwrap();
        assert!(
            config
                .tools()
                .iter()
                .any(|t| t.id == "top" && t.key == Some('p'))
        );
        let spike = config
            .templates()
            .into_iter()
            .find(|t| t.id == "spike")
            .unwrap();
        assert!(spike.lead_prompt.starts_with("You lead the spike"));
        assert!(spike.child_prompt.contains("{duty}"));
        assert_eq!(
            config.permissions.mode_for(&"codex".into()),
            Some(PermissionMode::Plan)
        );
    }

    #[test]
    fn mistakes_are_reported() {
        let unknown = Config::from_toml("[terminal]\nfont-sise = 13").unwrap_err();
        assert!(unknown.to_string().contains("font-sise"), "{unknown}");
        let range = Config::from_toml("[terminal]\nfont-size = 200").unwrap_err();
        assert!(range.to_string().contains("font-size"), "{range}");
        let color = Config::from_toml("[appearance]\naccent = \"blue\"").unwrap_err();
        assert!(color.to_string().contains("blue"), "{color}");
        let twice = "[[agents]]\nid = \"a\"\nlabel = \"A\"\n[[agents]]\nid = \"a\"\nlabel = \"B\"";
        assert!(Config::from_toml(twice).is_err());
    }

    #[test]
    fn agents_extend_the_builtin_presets() {
        let config = Config::from_toml(
            "[[agents]]\nid = \"claude-yolo\"\nlabel = \"Claude (yolo)\"\ncommand = \"claude --dangerously-skip-permissions\"",
        )
        .unwrap();
        let presets = config.presets();
        assert_eq!(presets.last().unwrap().id(), "claude-yolo");
        assert_eq!(presets.last().unwrap().base_name(), "claude");
    }

    #[test]
    fn templates_extend_the_builtins() {
        let config = Config::from_toml(
            "[[templates]]\nid = \"review-team\"\nlabel = \"Review team\"\nkind = \"review\"\nchildren = 2\nchild = \"codex\"\nroles = [\"reviewer\"]\nquestions = [\"What to review\"]",
        )
        .unwrap();
        let template = config
            .templates()
            .into_iter()
            .find(|t| t.id == "review-team")
            .unwrap();
        assert_eq!(template.kind, Some(WorkKind::Review));
        assert_eq!(template.team_summary(), "lead + reviewer + implementer");
        assert_eq!(template.children[0].preset, Some("codex".into()));
        let unknown =
            Config::from_toml("[[templates]]\nid = \"x\"\nlabel = \"X\"\nlead = \"nope\"")
                .unwrap_err();
        assert!(unknown.to_string().contains("nope"), "{unknown}");
    }
}
