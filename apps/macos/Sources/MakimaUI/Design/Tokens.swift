import AppKit
import MakimaCore

/// Colours derived from the terminal theme by the Rust core (see
/// `docs/design-system.md`). The TUI chrome itself is drawn with ANSI palette
/// colours by `makima-tui`; these cover what AppKit paints around it: the
/// window background and the veil over unfocused panes.
nonisolated struct Tokens: Equatable, Sendable {
    struct Surface: Equatable {
        /// The window and terminal background.
        var base: NSColor
    }

    var isDark: Bool
    var surface: Surface
    /// Opacity of the veil over unfocused panes.
    var unfocusedDim: CGFloat = 0.2
}

nonisolated extension Tokens {
    init(_ derived: DesignTokens, unfocusedDim: CGFloat) {
        self.init(
            isDark: derived.isDark,
            surface: Surface(base: NSColor(hex: derived.surfaceBase)),
            unfocusedDim: unfocusedDim
        )
    }

    /// Neutral dark tokens for the instant before the real ones load.
    nonisolated static let fallback = Tokens(
        isDark: true,
        surface: Surface(base: NSColor(hex: "#101010"))
    )
}

extension NSColor {
    /// `#rrggbb` (or `#rgb`) in sRGB; malformed input yields magenta so it's
    /// impossible to miss.
    nonisolated convenience init(hex: String) {
        var digits = hex.hasPrefix("#") ? String(hex.dropFirst()) : hex
        if digits.count == 3 { digits = digits.map { "\($0)\($0)" }.joined() }
        guard digits.count == 6, let value = UInt32(digits, radix: 16) else {
            self.init(srgbRed: 1, green: 0, blue: 1, alpha: 1)
            return
        }
        self.init(
            srgbRed: CGFloat((value >> 16) & 0xff) / 255,
            green: CGFloat((value >> 8) & 0xff) / 255,
            blue: CGFloat(value & 0xff) / 255,
            alpha: 1
        )
    }
}
