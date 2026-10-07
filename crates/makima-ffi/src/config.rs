//! Loading the user's config file for the app.

use std::io::ErrorKind;
use std::path::PathBuf;

use makima_core::config::{self, Config, CursorStyle, Leader, Navigate, VoiceProvider};

use crate::{AgentPresetInfo, KeyStyle};

/// The key from an agent into Makima's keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum LeaderKey {
    CmdEsc,
    CtrlSpace,
    CtrlO,
    CtrlBracket,
    ShiftEsc,
}

/// The modifier that, with `h j k l`, moves between panes while typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NavigateKeys {
    Ctrl,
    Alt,
    Off,
}

/// A tool opened in a floating window over a work, in its folder.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ToolInfo {
    pub id: String,
    pub label: String,
    /// `None` runs the login shell.
    pub command: Option<String>,
    /// One character, or `None` for the tools menu only.
    pub key: Option<String>,
    /// Share of the screen, 30 to 100.
    pub percent: u8,
    /// Keep running while hidden.
    pub keep: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AppearanceMode {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AppearanceSettings {
    pub mode: AppearanceMode,
    pub light_theme: String,
    pub dark_theme: String,
    /// `#rrggbb`, replacing the theme's accent.
    pub accent: Option<String>,
    pub contrast: f32,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct TerminalSettings {
    pub font_family: Option<String>,
    pub font_size: f32,
    pub padding_x: u16,
    pub padding_y: u16,
    pub unfocused_dim: f32,
    pub cursor_style: CursorShape,
    pub cursor_blink: bool,
    /// The shell for shell panes and tools; `None` lets the app choose.
    pub shell: Option<String>,
}

/// A terminal cursor's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum CursorShape {
    Block,
    Bar,
    Underline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum VoiceProviderKind {
    /// macOS 26's on-device `SpeechAnalyzer`.
    Apple,
    /// An OpenAI-compatible transcription API.
    Openai,
    Off,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VoiceSettings {
    pub provider: VoiceProviderKind,
    /// BCP 47, e.g. `en-US`; the system language when unset.
    pub language: Option<String>,
    /// For `Openai`: the model, the API's base URL, and the environment
    /// variable holding the key.
    pub model: String,
    pub base_url: String,
    pub api_key_env: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ConfigInfo {
    pub appearance: AppearanceSettings,
    pub terminal: TerminalSettings,
    pub voice: VoiceSettings,
    pub key_style: KeyStyle,
    pub navigate: NavigateKeys,
    pub leader: LeaderKey,
    /// Built-in tools with the config's `[[tools]]` applied, in menu order.
    pub tools: Vec<ToolInfo>,
    /// Built-in presets with the config's `[[agents]]` applied.
    pub presets: Vec<AgentPresetInfo>,
}

/// The config the app should use, and anything wrong with the file.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct LoadedConfig {
    pub config: ConfigInfo,
    /// The file that was (or would be) read.
    pub path: Option<String>,
    /// Why the file couldn't be used; the defaults apply when set. A
    /// missing file is not a problem.
    pub problem: Option<String>,
}

/// A commented config with every default, for creating the file.
#[uniffi::export]
#[must_use]
pub fn default_config_text() -> String {
    config::DEFAULT_CONFIG.to_owned()
}

/// Reads `path`, or the default config location when `None`. Never fails:
/// problems are reported in [`LoadedConfig::problem`] and the defaults used.
#[uniffi::export]
#[must_use]
pub fn load_config(path: Option<String>) -> LoadedConfig {
    let (config, path, problem) = read_config(path.map(PathBuf::from));
    LoadedConfig {
        config: (&config).into(),
        path: path.map(|p| p.display().to_string()),
        problem,
    }
}

/// The config at `path` (or the default location), the path it came from,
/// and why it couldn't be used, if it couldn't.
pub(crate) fn read_config(path: Option<PathBuf>) -> (Config, Option<PathBuf>, Option<String>) {
    let path = path.or_else(|| {
        makima_store::Paths::resolve(None)
            .ok()
            .map(|p| p.config_file)
    });
    let Some(path) = path else {
        return (Config::default(), None, None);
    };
    let (config, problem) = match std::fs::read_to_string(&path) {
        Ok(text) => match Config::from_toml(&text) {
            Ok(config) => (config, None),
            Err(error) => (
                Config::default(),
                Some(format!("{}: {error}", path.display())),
            ),
        },
        Err(error) if error.kind() == ErrorKind::NotFound => (Config::default(), None),
        Err(error) => (
            Config::default(),
            Some(format!("{}: {error}", path.display())),
        ),
    };
    (config, Some(path), problem)
}

impl From<&Config> for ConfigInfo {
    fn from(config: &Config) -> Self {
        let a = &config.appearance;
        let t = &config.terminal;
        Self {
            appearance: AppearanceSettings {
                mode: match a.mode {
                    config::AppearanceMode::Light => AppearanceMode::Light,
                    config::AppearanceMode::Dark => AppearanceMode::Dark,
                    // System, and any mode added to the core later.
                    _ => AppearanceMode::System,
                },
                light_theme: a.theme.light().to_owned(),
                dark_theme: a.theme.dark().to_owned(),
                accent: a.accent.map(|c| c.to_string()),
                contrast: a.contrast,
            },
            terminal: TerminalSettings {
                font_family: t.font_family.clone(),
                font_size: t.font_size,
                padding_x: t.padding_x,
                padding_y: t.padding_y,
                unfocused_dim: t.unfocused_dim,
                cursor_style: match t.cursor_style {
                    CursorStyle::Bar => CursorShape::Bar,
                    CursorStyle::Underline => CursorShape::Underline,
                    // Block, and any shape added later.
                    _ => CursorShape::Block,
                },
                cursor_blink: t.cursor_blink,
                shell: t.shell.clone(),
            },
            voice: VoiceSettings {
                provider: match config.voice.provider {
                    VoiceProvider::None => VoiceProviderKind::Off,
                    VoiceProvider::Openai => VoiceProviderKind::Openai,
                    // Apple, and any provider added to the core later.
                    _ => VoiceProviderKind::Apple,
                },
                language: config.voice.language.clone(),
                model: config.voice.model.clone(),
                base_url: config.voice.base_url.clone(),
                api_key_env: config.voice.api_key_env.clone(),
            },
            key_style: config.keys.style.into(),
            leader: match config.keys.leader {
                Leader::CtrlO => LeaderKey::CtrlO,
                Leader::CtrlBracket => LeaderKey::CtrlBracket,
                Leader::ShiftEsc => LeaderKey::ShiftEsc,
                Leader::CmdEsc => LeaderKey::CmdEsc,
                // Ctrl+Space, and any key added later.
                _ => LeaderKey::CtrlSpace,
            },
            navigate: match config.keys.navigate {
                Navigate::Alt => NavigateKeys::Alt,
                Navigate::Off => NavigateKeys::Off,
                // Ctrl, and any style added later.
                _ => NavigateKeys::Ctrl,
            },
            tools: config
                .tools()
                .into_iter()
                .map(|t| ToolInfo {
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "Validated to 0.3–1, so this fits"
                    )]
                    percent: (t.size * 100.0).round() as u8,
                    key: t.key.map(String::from),
                    id: t.id,
                    label: t.label,
                    command: t.command,
                    keep: t.keep,
                })
                .collect(),
            presets: config.presets().iter().map(Into::into).collect(),
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use super::*;

    /// `text` in a config file that lives as long as the returned handle.
    fn temp_config(text: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, text).unwrap();
        (dir, path.display().to_string())
    }

    #[test]
    fn missing_files_use_defaults_quietly() {
        let loaded = load_config(Some("/nonexistent/makima/config.toml".into()));
        assert_eq!(loaded.problem, None);
        assert_eq!(loaded.config.appearance.dark_theme, "Vesper");
        assert_eq!(loaded.config.key_style, KeyStyle::Helix);
        assert_eq!(loaded.config.leader, LeaderKey::CtrlSpace);
        let git = &loaded.config.tools[0];
        assert_eq!(
            (git.label.as_str(), git.key.as_deref(), git.percent),
            ("lazygit", Some("g"), 90)
        );
    }

    #[test]
    fn broken_files_report_and_fall_back() {
        let (_dir, path) = temp_config("[terminal]\nfont-size = 500\n");
        let loaded = load_config(Some(path));
        assert!(loaded.problem.unwrap().contains("font-size"));
        assert!((loaded.config.terminal.font_size - 14.0).abs() < f32::EPSILON);
        assert_eq!(loaded.config.terminal.cursor_style, CursorShape::Block);
        assert!(
            !loaded.config.terminal.cursor_blink,
            "a steady cursor by default"
        );
    }

    #[test]
    fn valid_files_apply() {
        let (_dir, path) = temp_config(
            "[keys]\nstyle = \"tmux\"\n[appearance]\ntheme = \"Rose Pine\"\naccent = \"#7aa2f7\"\n[[agents]]\nid = \"aider\"\nlabel = \"Aider\"\ncommand = \"aider\"\n",
        );
        let loaded = load_config(Some(path));
        assert_eq!(loaded.problem, None);
        assert_eq!(loaded.config.appearance.light_theme, "Rose Pine");
        assert_eq!(loaded.config.appearance.accent.as_deref(), Some("#7aa2f7"));
        assert_eq!(loaded.config.presets.last().unwrap().id, "aider");
        assert_eq!(loaded.config.key_style, KeyStyle::Tmux);
    }
}
