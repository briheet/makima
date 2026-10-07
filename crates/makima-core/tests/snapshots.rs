//! Snapshot tests: the derived design tokens and the default config, so any
//! change to the colour rules or config shape shows up as a reviewable diff.
//! Update with `cargo insta review` (or `INSTA_UPDATE=always cargo test`).

use std::fmt::Write;

use makima_core::color::Rgb;
use makima_core::config::{Config, DEFAULT_CONFIG};
use makima_core::tokens::{DesignTokens, ThemePalette, TokenOptions};

fn hex(s: &str) -> Rgb {
    s.parse().unwrap_or_else(|e| panic!("{e}"))
}

/// Vesper's colours, from the Ghostty theme catalog.
fn vesper() -> ThemePalette {
    let ansi = [
        "#101010", "#f5a191", "#90b99f", "#e6b99d", "#aca1cf", "#e29eca", "#ea83a5", "#a0a0a0",
        "#7e7e7e", "#ff8080", "#99ffe4", "#ffc799", "#b9aeda", "#ecaad6", "#f591b2", "#ffffff",
    ]
    .map(hex);
    ThemePalette::new(hex("#101010"), hex("#ffffff"), Some(hex("#acb1ab")), ansi)
}

/// GitHub Light Default's colours.
fn github_light() -> ThemePalette {
    let ansi = [
        "#24292f", "#cf222e", "#116329", "#4d2d00", "#0969da", "#8250df", "#1b7c83", "#6e7781",
        "#57606a", "#a40e26", "#1a7f37", "#633c01", "#218bff", "#a475f9", "#3192aa", "#8c959f",
    ]
    .map(hex);
    ThemePalette::new(hex("#ffffff"), hex("#1f2328"), Some(hex("#0969da")), ansi)
}

fn table(tokens: &DesignTokens) -> String {
    let mut out = String::new();
    let mut row = |name: &str, color: Rgb| {
        writeln!(out, "{name:<20} {color}").unwrap_or_else(|e| panic!("{e}"));
    };
    row("surface.base", tokens.surface.base);
    row("surface.panel", tokens.surface.panel);
    row("surface.elevated", tokens.surface.elevated);
    row("surface.inset", tokens.surface.inset);
    row("border.subtle", tokens.border.subtle);
    row("border.default", tokens.border.default);
    row("border.strong", tokens.border.strong);
    row("border.focus", tokens.border.focus);
    row("text.primary", tokens.text.primary);
    row("text.secondary", tokens.text.secondary);
    row("text.tertiary", tokens.text.tertiary);
    row("text.disabled", tokens.text.disabled);
    row("text.on-accent", tokens.text.on_accent);
    row("interactive.hover", tokens.interactive.hover);
    row("interactive.pressed", tokens.interactive.pressed);
    row("interactive.selected", tokens.interactive.selected);
    row("accent.base", tokens.accent.base);
    row("accent.muted", tokens.accent.muted);
    row("status.running", tokens.status.running);
    row("status.attention", tokens.status.attention);
    row("status.danger", tokens.status.danger);
    row("status.info", tokens.status.info);
    row("status.neutral", tokens.status.neutral);
    out
}

#[test]
fn vesper_tokens() {
    insta::assert_snapshot!(table(&DesignTokens::derive(
        &vesper(),
        TokenOptions::default()
    )));
}

#[test]
fn github_light_tokens() {
    insta::assert_snapshot!(table(&DesignTokens::derive(
        &github_light(),
        TokenOptions::default()
    )));
}

#[test]
fn default_config() {
    let config = Config::from_toml(DEFAULT_CONFIG).unwrap_or_else(|e| panic!("{e}"));
    insta::assert_debug_snapshot!(config);
}
