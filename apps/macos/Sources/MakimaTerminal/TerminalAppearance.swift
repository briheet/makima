import Foundation
import GhosttyTerminal
import GhosttyTheme

/// The primitive colours of a terminal theme, as `#rrggbb` strings: the
/// input to Makima's design tokens.
public struct ThemeColors: Sendable, Equatable {
    public let name: String
    public let background: String
    public let foreground: String
    public let cursor: String?
    /// ANSI colours 0–15.
    public let ansi: [String]
}

/// Ghostty's theme catalog (iTerm2-Color-Schemes, 485 themes).
public enum ThemeCatalog {
    /// The theme called `name`, matched case-insensitively.
    public static func colors(named name: String) -> ThemeColors? {
        definition(named: name).map(ThemeColors.init)
    }

    static func definition(named name: String) -> GhosttyThemeDefinition? {
        GhosttyThemeCatalog.theme(named: name)
            ?? GhosttyThemeCatalog.search(name).first { $0.name.caseInsensitiveCompare(name) == .orderedSame }
    }
}

extension ThemeColors {
    init(_ theme: GhosttyThemeDefinition) {
        let hex = { (value: String) in value.hasPrefix("#") ? value : "#" + value }
        name = theme.name
        background = hex(theme.background)
        foreground = hex(theme.foreground)
        cursor = theme.cursorColor.map(hex)
        ansi = (0..<16).map { theme.palette[$0].map(hex) ?? hex(theme.foreground) }
    }
}

/// Font and spacing for every terminal; colours come from the theme.
public struct TerminalStyle: Sendable, Equatable {
    /// A cursor's shape where the program doesn't set its own.
    public enum Cursor: String, Sendable {
        case block, bar, underline
    }

    public var fontFamily: String?
    public var fontSize: Float
    public var paddingX: Int
    public var paddingY: Int
    public var cursor: Cursor
    public var cursorBlinks: Bool

    public init(
        fontFamily: String?,
        fontSize: Float,
        paddingX: Int,
        paddingY: Int,
        cursor: Cursor = .block,
        cursorBlinks: Bool = false
    ) {
        self.fontFamily = fontFamily
        self.fontSize = fontSize
        self.paddingX = paddingX
        self.paddingY = paddingY
        self.cursor = cursor
        self.cursorBlinks = cursorBlinks
    }

    /// The same font and cursor with no padding, for the chrome terminal:
    /// its cell (0, 0) must sit exactly at its view's corner.
    public var chrome: TerminalStyle {
        var chrome = self
        chrome.paddingX = 0
        chrome.paddingY = 0
        return chrome
    }
}

/// Builds and updates the one libghostty controller every terminal shares.
public enum TerminalAppearance {
    /// Shortcuts that belong to Makima's menus. Ghostty binds some of these
    /// itself (new tab, close surface, …), and a bound key never reaches the
    /// menu, so they are unbound.
    static let appShortcuts = [
        "super+n", "super+t", "super+shift+t", "super+w", "super+r", "super+comma", "super+shift+comma",
        "super+enter", "super+bracket_left", "super+bracket_right", "super+shift+b", "super+l",
        "super+shift+l", "super+semicolon", "super+slash", "ctrl+space", "super+escape",
    ]

    public static func makeController(
        settings: TerminalStyle,
        light: ThemeColors,
        dark: ThemeColors
    ) -> TerminalController {
        let controller = TerminalController(
            configuration: TerminalConfiguration(), theme: theme(light: light, dark: dark))
        controller.setTerminalConfiguration(configuration(settings))
        return controller
    }

    /// Applies new settings and themes to live terminals.
    public static func update(
        _ controller: TerminalController,
        settings: TerminalStyle,
        light: ThemeColors,
        dark: ThemeColors
    ) {
        controller.setTheme(theme(light: light, dark: dark))
        controller.setTerminalConfiguration(configuration(settings))
    }

    private static func theme(light: ThemeColors, dark: ThemeColors) -> TerminalTheme {
        let config = { (colors: ThemeColors) in
            ThemeCatalog.definition(named: colors.name)?.toTerminalConfiguration() ?? TerminalConfiguration()
        }
        return TerminalTheme(light: config(light), dark: config(dark))
    }

    private static func configuration(_ settings: TerminalStyle) -> TerminalConfiguration {
        TerminalConfiguration { config in
            if let family = settings.fontFamily { config.withFontFamily(family) }
            config.withFontSize(settings.fontSize)
            config.withWindowPaddingX(settings.paddingX)
            config.withWindowPaddingY(settings.paddingY)
            config.withCursorStyle(TerminalCursorStyle(rawValue: settings.cursor.rawValue) ?? .block)
            config.withCursorStyleBlink(settings.cursorBlinks)
            for shortcut in appShortcuts {
                config.withCustom("keybind", "\(shortcut)=unbind")
            }
        }
    }
}
