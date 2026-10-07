# Makima design system

Research date: 2026-10-06

> **Superseded for the window chrome by `tui-design.md`.** The chrome is now a TUI drawn with the terminal theme's ANSI palette, in the terminal font, so the type scale, spacing scale and `ui-scale` setting below no longer apply (`ui-scale` was removed from the config). The token derivation is still used, for the window background and the veil over unfocused panes, and stays the reference for any non-terminal UI.

Makima should look like a terminal that grew a little chrome, not like an app with terminals in it. The terminals are the content; everything else stays quiet, follows the terminal theme, and gets out of the way.

## Principles

1. **The terminal theme is the source of truth.** The window, sidebar, headers and controls are derived from the active Ghostty theme, so the chrome and the terminals always match. This is what Ghostty's own `window-theme = auto` does.
2. **Few inputs, many tokens.** Like Linear's theme system, every colour is generated from a handful of inputs: the theme's background, foreground and ANSI palette, an optional accent, and a contrast level. Nobody hand-picks 50 colours.
3. **Perceptual colour math.** Colours are derived in OKLab/OKLCH, where equal lightness steps look equal across hues. HSL and plain RGB mixing don't have that property.
4. **Contrast is enforced, not hoped for.** Derived text and status colours are nudged until they meet minimum contrast against the background (7:1 for primary text, 4.5:1 for secondary, 3:1 for tertiary text and status colours), even on low-contrast themes.
5. **Three token tiers.** Primitives (theme colours) feed semantic tokens (`surface.panel`, `text.secondary`, …), which views use. Views never use a primitive or a literal colour.
6. **Quiet chrome.** Use hairline borders rather than boxes, and dim unfocused terminals rather than outlining them (Ghostty's `unfocused-split-opacity`). Colour is reserved for state: status dots, focus, the accent.
7. **One type family, small scale.** The system font (SF Pro) for UI, monospaced for anything that comes from the terminal world (paths, commands, shortcuts). One `ui-scale` setting scales the whole UI.
8. **A 4pt spacing grid.** All padding, gaps and sizes come from the spacing scale.

## Token architecture

```text
Ghostty theme (primitives)        Semantic tokens                 Views
background, foreground,   ──▶     surface.base / panel /   ──▶   sidebar, tile header,
cursor, selection,                elevated / inset               start screen, buttons
ansi[0…15]                        border.subtle / default /
+ accent, contrast                strong / focus
                                  text.primary / secondary /
                                  tertiary / disabled / on-accent
                                  interactive.hover / pressed /
                                  selected
                                  accent.base / muted
                                  status.running / attention /
                                  danger / info / neutral
```

The derivation lives in the Rust core (`makima-core::tokens`), so it's unit-tested (contrast gates included) and shared with the CLI. Swift looks the theme up in libghostty's `GhosttyTheme` catalog, passes its colours to Rust once, and gets semantic tokens back.

### How tokens are derived

Let `k = 0.6 + 0.8 × contrast`, with `contrast` in 0…1 and a default of 0.5 (so `k = 1`). "Toward the foreground" means lighter on dark themes and darker on light themes.

| Token | Derivation |
|---|---|
| `surface.base` | theme background (also the terminal background) |
| `surface.panel` | background lightness shifted 0.03·k toward the foreground; chroma capped so it stays neutral |
| `surface.elevated` | shifted 0.06·k |
| `surface.inset` | 4%·k mix toward the foreground |
| `border.subtle / default / strong` | 7% / 12% / 22% (×k) mix of background → foreground |
| `text.primary` | foreground, gated to 7:1 |
| `text.secondary / tertiary / disabled` | 70% / 48% / 32% mix, gated to 4.5:1 / 3:1 / none |
| `interactive.hover / pressed / selected` | 5% / 8% / 10% (×k) mix |
| `accent.base` | config `accent`, else the theme cursor colour if it has colour, else ANSI blue; gated to 3:1 |
| `accent.muted` | 18% mix of background → accent |
| `status.*` | ANSI green (running), yellow (attention), red (danger), blue (info), gated to 3:1; neutral = tertiary text |

All mixes happen in OKLab. Lightness changes keep hue and reduce chroma if needed to stay inside sRGB.

## Typography

System font; sizes are multiplied by `appearance.ui-scale`.

| Style | Size / weight | Used for |
|---|---|---|
| `caption` | 10 semibold, tracked | section labels ("WORK") |
| `footnote` | 11 regular | metadata, hints |
| `body` | 12 regular | rows, buttons |
| `bodyStrong` | 12 semibold | agent names in tile headers |
| `label` | 13 medium | buttons, chips |
| `title` | 15 regular | input text |
| `display` | 24 semibold | the start screen question |
| `mono` | 11 monospaced | paths, commands, shortcuts |

## Spacing, radius, motion

- **Spacing:** 2, 4, 8, 12, 16, 24, 32, 40.
- **Radius:** 4 (small controls), 6 (rows, chips), 8 (tiles, fields), 12 (dialogs).
- **Motion:** 120ms ease-out for hover and selection, 180ms for layout. No motion when *Reduce Motion* is on.
- **Sizes:** sidebar 184pt (160–260), tile header 28pt, grid gap 6pt.

## Configuration

`~/.config/makima/config.toml` (or `$XDG_CONFIG_HOME/makima/config.toml`) is parsed by the Rust core with serde and strictly validated: unknown keys are errors, and numbers are range-checked. `makima config init` writes a commented default, and `makima config check` validates it. In the app, *Reload Config* (⌘⇧,) applies changes live.

```toml
[appearance]
mode = "system"                 # system | light | dark
theme = { light = "GitHub Light Default", dark = "Vesper" }   # or theme = "Rose Pine"
# accent = "#7aa2f7"
contrast = 0.5                  # 0 = softest, 1 = strongest
ui-scale = 1.0

[terminal]
# font-family = "JetBrains Mono"
font-size = 12
padding-x = 10
padding-y = 8
unfocused-dim = 0.2             # dim unfocused terminals (0 = off)

[[agents]]                      # add or replace presets by id
id = "claude-work"
label = "Claude (work account)"
adapter = "claude-code"         # knows Claude's sessions and flags
command = "claude-work"
```

Theme names come from libghostty's catalog of 485 themes (iTerm2-Color-Schemes). An unknown name falls back to the default and is reported on the start screen.

## Sources

- [How we redesigned the Linear UI (Linear)](https://linear.app/now/how-we-redesigned-the-linear-ui)
- [Design tokens architecture: the three-tier system](https://www.productdesign.vn/en/articles/design-tokens-architecture-2026/)
- [Design tokens and theming architecture](https://sujeet.pro/articles/design-tokens-and-theming)
- [OKLCH in CSS: why we moved from RGB and HSL (Evil Martians)](https://evilmartians.com/chronicles/oklch-in-css-why-quit-rgb-hsl)
- [Exploring the OKLCH ecosystem (Evil Martians)](https://evilmartians.com/chronicles/exploring-the-oklch-ecosystem-and-its-tools)
- [OKLCH + APCA colour systems](https://accessibility.build/guides/oklch-apca-color-systems)
- [Oklab colour space](https://en.wikipedia.org/wiki/Oklab_color_space)
- [ColorTokensKit-Swift (LCH/OKLCH tokens for Swift)](https://github.com/metasidd/ColorTokensKit-Swift)
- [SwiftUI dark mode: design tokens & theming](https://theswiftk.it.com/blog/swiftui-dark-mode-design-tokens-guide)
- [Designing a scalable app-wide theming system in SwiftUI](https://www.sagarunagar.com/blog/app-wide-theming-swiftui)
- [Zed themes](https://zed.dev/docs/themes)
- [Ghostty config reference](https://ghostty.org/docs/config/reference)
- [Get to know the new design system (WWDC25)](https://developer.apple.com/videos/play/wwdc2025/356/)
- [Raycast colours API](https://developers.raycast.com/api-reference/user-interface/colors)
