//! Design tokens for the app, derived from a terminal theme's colours.
//! Colours cross the FFI as `#rrggbb` strings.

use makima_core::color::Rgb;
use makima_core::tokens::{DesignTokens, ThemePalette, TokenOptions};

use crate::CoreError;

/// The colours of a terminal theme, as `#rrggbb` (a leading `#` is optional).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ThemePaletteInfo {
    pub background: String,
    pub foreground: String,
    pub cursor: Option<String>,
    /// ANSI colours 0–15; missing entries are filled from the foreground.
    pub ansi: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SurfaceTokens {
    pub base: String,
    pub panel: String,
    pub elevated: String,
    pub inset: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BorderTokens {
    pub subtle: String,
    pub default: String,
    pub strong: String,
    pub focus: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TextTokens {
    pub primary: String,
    pub secondary: String,
    pub tertiary: String,
    pub disabled: String,
    pub on_accent: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct InteractiveTokens {
    pub hover: String,
    pub pressed: String,
    pub selected: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AccentTokens {
    pub base: String,
    pub muted: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StatusTokens {
    pub running: String,
    pub attention: String,
    pub danger: String,
    pub info: String,
    pub neutral: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DesignTokensInfo {
    pub is_dark: bool,
    pub surface: SurfaceTokens,
    pub border: BorderTokens,
    pub text: TextTokens,
    pub interactive: InteractiveTokens,
    pub accent: AccentTokens,
    pub status: StatusTokens,
}

/// Derives every UI colour from a theme. See `docs/design-system.md`.
///
/// # Errors
/// [`CoreError::InvalidColor`] if a colour isn't `#rrggbb` or `#rgb`.
#[uniffi::export]
#[expect(
    clippy::needless_pass_by_value,
    reason = "UniFFI passes records and optionals by value"
)]
pub fn derive_design_tokens(
    palette: ThemePaletteInfo,
    contrast: f32,
    accent: Option<String>,
) -> Result<DesignTokensInfo, CoreError> {
    let foreground = parse(&palette.foreground)?;
    let mut ansi = [foreground; 16];
    for (slot, value) in ansi.iter_mut().zip(&palette.ansi) {
        *slot = parse(value)?;
    }
    let theme = ThemePalette::new(
        parse(&palette.background)?,
        foreground,
        palette.cursor.as_deref().map(parse).transpose()?,
        ansi,
    );
    let options = TokenOptions::default()
        .with_contrast(contrast)
        .with_accent(accent.as_deref().map(parse).transpose()?);
    Ok(DesignTokens::derive(&theme, options).into())
}

fn parse(value: &str) -> Result<Rgb, CoreError> {
    value.parse().map_err(|_| CoreError::InvalidColor {
        value: value.to_owned(),
    })
}

impl From<DesignTokens> for DesignTokensInfo {
    fn from(t: DesignTokens) -> Self {
        let hex = |c: Rgb| c.to_string();
        Self {
            is_dark: t.is_dark,
            surface: SurfaceTokens {
                base: hex(t.surface.base),
                panel: hex(t.surface.panel),
                elevated: hex(t.surface.elevated),
                inset: hex(t.surface.inset),
            },
            border: BorderTokens {
                subtle: hex(t.border.subtle),
                default: hex(t.border.default),
                strong: hex(t.border.strong),
                focus: hex(t.border.focus),
            },
            text: TextTokens {
                primary: hex(t.text.primary),
                secondary: hex(t.text.secondary),
                tertiary: hex(t.text.tertiary),
                disabled: hex(t.text.disabled),
                on_accent: hex(t.text.on_accent),
            },
            interactive: InteractiveTokens {
                hover: hex(t.interactive.hover),
                pressed: hex(t.interactive.pressed),
                selected: hex(t.interactive.selected),
            },
            accent: AccentTokens {
                base: hex(t.accent.base),
                muted: hex(t.accent.muted),
            },
            status: StatusTokens {
                running: hex(t.status.running),
                attention: hex(t.status.attention),
                danger: hex(t.status.danger),
                info: hex(t.status.info),
                neutral: hex(t.status.neutral),
            },
        }
    }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap fails the test")]
mod tests {
    use super::*;

    #[test]
    fn derives_from_hex_without_hashes() {
        let tokens = derive_design_tokens(
            ThemePaletteInfo {
                background: "101010".into(),
                foreground: "ffffff".into(),
                cursor: None,
                ansi: vec!["000000".into(); 16],
            },
            0.5,
            Some("#7aa2f7".into()),
        )
        .unwrap();
        assert!(tokens.is_dark);
        assert_eq!(tokens.surface.base, "#101010");
        assert_eq!(tokens.accent.base, "#7aa2f7");
    }

    #[test]
    fn bad_colours_are_errors() {
        let error = derive_design_tokens(
            ThemePaletteInfo {
                background: "nope".into(),
                foreground: "#fff".into(),
                cursor: None,
                ansi: Vec::new(),
            },
            0.5,
            None,
        )
        .unwrap_err();
        assert!(matches!(error, CoreError::InvalidColor { .. }));
    }
}
