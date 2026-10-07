//! Semantic design tokens derived from a terminal theme.
//!
//! A handful of inputs — the theme's background, foreground and ANSI
//! palette, an optional accent and a contrast level — produce every colour
//! the UI uses. See `docs/design-system.md` for the rules.

use crate::color::{Rgb, ensure_contrast};

/// The primitive colours of a terminal theme.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ThemePalette {
    pub background: Rgb,
    pub foreground: Rgb,
    pub cursor: Option<Rgb>,
    /// ANSI colours 0–15.
    pub ansi: [Rgb; 16],
}

/// User adjustments applied on top of a theme.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct TokenOptions {
    /// 0 (softest) to 1 (strongest); 0.5 is the designed default.
    pub contrast: f32,
    /// Replaces the accent picked from the theme.
    pub accent: Option<Rgb>,
}

impl ThemePalette {
    #[must_use]
    pub fn new(background: Rgb, foreground: Rgb, cursor: Option<Rgb>, ansi: [Rgb; 16]) -> Self {
        Self {
            background,
            foreground,
            cursor,
            ansi,
        }
    }
}

impl TokenOptions {
    /// `contrast` is clamped to 0–1.
    #[must_use]
    pub fn with_contrast(self, contrast: f32) -> Self {
        Self {
            contrast: contrast.clamp(0.0, 1.0),
            ..self
        }
    }

    #[must_use]
    pub fn with_accent(self, accent: Option<Rgb>) -> Self {
        Self { accent, ..self }
    }
}

impl Default for TokenOptions {
    fn default() -> Self {
        Self {
            contrast: 0.5,
            accent: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Surfaces {
    /// The window and terminal background.
    pub base: Rgb,
    /// Sidebar and tile headers.
    pub panel: Rgb,
    /// Popovers and overlays.
    pub elevated: Rgb,
    /// Input fields and chips.
    pub inset: Rgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Borders {
    pub subtle: Rgb,
    pub default: Rgb,
    pub strong: Rgb,
    pub focus: Rgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct TextColors {
    pub primary: Rgb,
    pub secondary: Rgb,
    pub tertiary: Rgb,
    pub disabled: Rgb,
    /// Text drawn on an `accent.base` fill.
    pub on_accent: Rgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Interactive {
    pub hover: Rgb,
    pub pressed: Rgb,
    pub selected: Rgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Accent {
    pub base: Rgb,
    /// A tinted background for accent-coloured content.
    pub muted: Rgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct StatusColors {
    pub running: Rgb,
    pub attention: Rgb,
    pub danger: Rgb,
    pub info: Rgb,
    pub neutral: Rgb,
}

/// Every semantic colour the UI uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct DesignTokens {
    pub is_dark: bool,
    pub surface: Surfaces,
    pub border: Borders,
    pub text: TextColors,
    pub interactive: Interactive,
    pub accent: Accent,
    pub status: StatusColors,
}

/// Minimum contrast ratios against `surface.base`.
const PRIMARY_TEXT: f32 = 7.0;
const SECONDARY_TEXT: f32 = 4.5;
const TERTIARY_TEXT: f32 = 3.0;
const GRAPHICS: f32 = 3.0;
/// Surfaces stay near-neutral even on strongly tinted themes.
const MAX_SURFACE_CHROMA: f32 = 0.03;
/// Below this chroma a cursor colour is grey and makes a poor accent.
const MIN_ACCENT_CHROMA: f32 = 0.05;

impl DesignTokens {
    #[must_use]
    pub fn derive(palette: &ThemePalette, options: TokenOptions) -> Self {
        let bg = palette.background;
        let fg = palette.foreground;
        let k = 0.6 + 0.8 * options.contrast.clamp(0.0, 1.0);
        let is_dark = bg.luminance() < fg.luminance();
        let toward_fg = if is_dark { 1.0 } else { -1.0 };
        let mix = |t: f32| bg.mix(fg, t * k);
        let shift = |delta: f32| {
            let lch = bg.oklch();
            let lch = crate::color::Oklch {
                c: lch.c.min(MAX_SURFACE_CHROMA),
                ..lch
            };
            lch.with_lightness(lch.l + toward_fg * delta * k).to_rgb()
        };

        let accent = options
            .accent
            .or(palette.cursor.filter(|c| c.oklch().c >= MIN_ACCENT_CHROMA))
            .unwrap_or(palette.ansi[4]);
        let accent = ensure_contrast(accent, bg, GRAPHICS);
        let primary = ensure_contrast(fg, bg, PRIMARY_TEXT);
        let on_accent = if accent.contrast(bg) >= accent.contrast(primary) {
            bg
        } else {
            primary
        };
        let status = |ansi: usize| ensure_contrast(palette.ansi[ansi], bg, GRAPHICS);
        let tertiary = ensure_contrast(bg.mix(fg, 0.48), bg, TERTIARY_TEXT);

        Self {
            is_dark,
            surface: Surfaces {
                base: bg,
                panel: shift(0.03),
                elevated: shift(0.06),
                inset: mix(0.04),
            },
            border: Borders {
                subtle: mix(0.07),
                default: mix(0.12),
                strong: mix(0.22),
                focus: accent,
            },
            text: TextColors {
                primary,
                secondary: ensure_contrast(bg.mix(fg, 0.70), bg, SECONDARY_TEXT),
                tertiary,
                disabled: bg.mix(fg, 0.32),
                on_accent,
            },
            interactive: Interactive {
                hover: mix(0.05),
                pressed: mix(0.08),
                selected: mix(0.10),
            },
            accent: Accent {
                base: accent,
                muted: bg.mix(accent, 0.18),
            },
            status: StatusColors {
                running: status(2),
                attention: status(3),
                danger: status(1),
                info: status(4),
                neutral: tertiary,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Rgb {
        s.parse().unwrap_or_else(|e| panic!("{e}"))
    }

    fn palette(background: &str, foreground: &str) -> ThemePalette {
        let ansi = [
            "#000000", "#cc3333", "#33aa55", "#ccaa33", "#3366cc", "#aa33aa", "#33aaaa", "#cccccc",
            "#666666", "#ff5555", "#55ff55", "#ffff55", "#5555ff", "#ff55ff", "#55ffff", "#ffffff",
        ]
        .map(hex);
        ThemePalette {
            background: hex(background),
            foreground: hex(foreground),
            cursor: None,
            ansi,
        }
    }

    #[test]
    fn dark_themes_raise_surfaces_toward_the_foreground() {
        let tokens = DesignTokens::derive(&palette("#101010", "#ffffff"), TokenOptions::default());
        assert!(tokens.is_dark);
        let l = |c: Rgb| c.oklch().l;
        assert!(l(tokens.surface.base) < l(tokens.surface.panel));
        assert!(l(tokens.surface.panel) < l(tokens.surface.elevated));
    }

    #[test]
    fn light_themes_darken_surfaces() {
        let tokens = DesignTokens::derive(&palette("#ffffff", "#1f2328"), TokenOptions::default());
        assert!(!tokens.is_dark);
        assert!(tokens.surface.panel.luminance() < tokens.surface.base.luminance());
    }

    #[test]
    fn low_contrast_themes_still_meet_the_gates() {
        // A deliberately muddy theme: grey on grey.
        let mut theme = palette("#3a3a3a", "#6a6a6a");
        theme.ansi[2] = hex("#405040");
        let tokens = DesignTokens::derive(&theme, TokenOptions::default());
        let base = tokens.surface.base;
        assert!(tokens.text.primary.contrast(base) >= PRIMARY_TEXT);
        assert!(tokens.text.secondary.contrast(base) >= SECONDARY_TEXT);
        assert!(tokens.text.tertiary.contrast(base) >= TERTIARY_TEXT);
        assert!(tokens.status.running.contrast(base) >= GRAPHICS);
    }

    #[test]
    fn accent_prefers_override_then_cursor_then_blue() {
        let mut theme = palette("#101010", "#ffffff");
        let blue = DesignTokens::derive(&theme, TokenOptions::default())
            .accent
            .base;
        assert_eq!(blue, theme.ansi[4]);

        theme.cursor = Some(hex("#f32882"));
        let cursor = DesignTokens::derive(&theme, TokenOptions::default())
            .accent
            .base;
        assert_eq!(cursor, hex("#f32882"));

        theme.cursor = Some(hex("#cccccc")); // grey: ignored
        let grey = DesignTokens::derive(&theme, TokenOptions::default())
            .accent
            .base;
        assert_eq!(grey, theme.ansi[4]);

        let options = TokenOptions {
            accent: Some(hex("#7aa2f7")),
            ..TokenOptions::default()
        };
        assert_eq!(
            DesignTokens::derive(&theme, options).accent.base,
            hex("#7aa2f7")
        );
    }

    #[test]
    fn contrast_option_widens_the_steps() {
        let theme = palette("#101010", "#ffffff");
        let soft = DesignTokens::derive(
            &theme,
            TokenOptions {
                contrast: 0.0,
                accent: None,
            },
        );
        let strong = DesignTokens::derive(
            &theme,
            TokenOptions {
                contrast: 1.0,
                accent: None,
            },
        );
        let base = theme.background;
        assert!(soft.border.default.contrast(base) < strong.border.default.contrast(base));
    }
}
