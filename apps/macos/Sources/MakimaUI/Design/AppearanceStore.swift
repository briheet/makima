import AppKit
import GhosttyTerminal
import MakimaCore
import MakimaTerminal
import Observation
import os

/// The app's look: the config, the resolved light and dark themes, the
/// design tokens derived from them, and the libghostty controller every
/// terminal shares. Follows macOS light/dark mode when configured to.
@Observable
public final class AppearanceStore {
    private(set) var config: AppConfig
    public private(set) var configPath: String?
    /// What's wrong with the config, for display. Defaults stand in for
    /// anything listed here.
    private(set) var problems: [String] = []
    /// Whether the dark theme is the one showing.
    private(set) var isDark: Bool
    /// The font size on screen: the config's, plus any zoom (⌘+ / ⌘−).
    private(set) var fontSize: Float
    /// Points added to the config's font size by zooming.
    @ObservationIgnored private var zoom: Float = 0
    @ObservationIgnored private var terminalStyle: TerminalStyle
    @ObservationIgnored private var themes: (light: ThemeColors, dark: ThemeColors)
    private var light: Tokens
    private var dark: Tokens

    /// The tokens for the theme on screen.
    var tokens: Tokens { isDark ? dark : light }

    @ObservationIgnored let terminalController: TerminalController
    /// The chrome terminal's controller: same theme and font, no padding, so
    /// its cell grid lines up exactly with the panes'.
    @ObservationIgnored let chromeController: TerminalController
    @ObservationIgnored private let logger = Logger(subsystem: "io.makima.app", category: "appearance")

    public init(loaded: AppConfig.Loaded) {
        let resolved = Self.resolve(loaded)
        config = loaded.config
        configPath = loaded.path
        problems = resolved.problems
        light = resolved.lightTokens
        dark = resolved.darkTokens
        isDark = resolved.darkTokens.isDark
        fontSize = resolved.terminal.fontSize
        terminalStyle = resolved.terminal
        themes = (resolved.lightTheme, resolved.darkTheme)
        terminalController = TerminalAppearance.makeController(
            settings: resolved.terminal,
            light: resolved.lightTheme,
            dark: resolved.darkTheme
        )
        chromeController = TerminalAppearance.makeController(
            settings: resolved.terminal.chrome,
            light: resolved.lightTheme,
            dark: resolved.darkTheme
        )
        applyMode()
    }

    /// Calls `apply` with the window background now and whenever it
    /// changes, for as long as the returned loop is kept.
    public func observeBackground(_ apply: @escaping @MainActor (NSColor) -> Void) -> ObservationLoop {
        ObservationLoop { [weak self] in
            guard let self else { return }
            apply(tokens.surface.base)
        }
    }

    /// Re-reads the config file and applies it to the UI and live terminals.
    public func reload() {
        let loaded = AppConfig.load(path: configPath)
        let resolved = Self.resolve(loaded)
        config = loaded.config
        problems = resolved.problems
        light = resolved.lightTokens
        dark = resolved.darkTokens
        terminalStyle = resolved.terminal
        themes = (resolved.lightTheme, resolved.darkTheme)
        applyTerminals()
        applyMode()
        logger.info("reloaded config; \(resolved.problems.count) problem(s)")
    }

    /// The shell for shell panes and the shell tool (see
    /// `LaunchCommand.preferredShell`).
    var shell: String {
        LaunchCommand.preferredShell(configured: config.terminal.shell)
    }

    /// Makes every terminal's text (chrome, agents, tools) `points` bigger,
    /// or smaller when negative, within 8 to 40 points.
    public func zoom(by points: Float) {
        let size = min(max(terminalStyle.fontSize + zoom + points, 8), 40)
        zoom = size - terminalStyle.fontSize
        applyTerminals()
    }

    /// Back to the config's font size.
    public func resetZoom() {
        zoom = 0
        applyTerminals()
    }

    /// Sends the font (with zoom) and themes to both controllers.
    private func applyTerminals() {
        var style = terminalStyle
        style.fontSize += zoom
        fontSize = style.fontSize
        TerminalAppearance.update(terminalController, settings: style, light: themes.light, dark: themes.dark)
        TerminalAppearance.update(chromeController, settings: style.chrome, light: themes.light, dark: themes.dark)
    }

    /// Forces the app's appearance when the config pins it, otherwise follows
    /// the system. A single theme for both modes pins the appearance to that
    /// theme's own lightness, like Ghostty's `window-theme = auto`.
    private func applyMode() {
        let appearance = config.appearance
        let pinned: Bool? =
            switch appearance.mode {
            case .light: false
            case .dark: true
            case .system: appearance.lightTheme == appearance.darkTheme ? dark.isDark : nil
            }
        NSApp.appearance = pinned.map { NSAppearance(named: $0 ? .darkAqua : .aqua) } ?? nil
        systemAppearanceDidChange()
    }

    /// Re-reads macOS light/dark mode. Called by a view's
    /// `viewDidChangeEffectiveAppearance`, which AppKit delivers on main.
    func systemAppearanceDidChange() {
        let systemDark = NSApp.effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        if isDark != systemDark { isDark = systemDark }
    }

    // MARK: - Resolution

    struct Resolved {
        var lightTheme: ThemeColors
        var darkTheme: ThemeColors
        var lightTokens: Tokens
        var darkTokens: Tokens
        var terminal: TerminalStyle
        var problems: [String]
    }

    /// Looks up the configured themes and derives tokens for each; anything
    /// unusable falls back to the defaults and is listed in `problems`.
    static func resolve(_ loaded: AppConfig.Loaded) -> Resolved {
        let appearance = loaded.config.appearance
        let terminal = loaded.config.terminal
        var problems = loaded.problem.map { [$0] } ?? []

        func theme(_ name: String, fallback: String) -> ThemeColors {
            if let colors = ThemeCatalog.colors(named: name) { return colors }
            problems.append("Unknown theme \"\(name)\"; using \(fallback).")
            guard let colors = ThemeCatalog.colors(named: fallback) else {
                preconditionFailure("default theme \(fallback) is missing from the catalog")
            }
            return colors
        }
        let lightTheme = theme(appearance.lightTheme, fallback: defaultLightTheme)
        let darkTheme = theme(appearance.darkTheme, fallback: defaultDarkTheme)

        func tokens(_ colors: ThemeColors) -> Tokens {
            let palette = ThemePalette(
                background: colors.background,
                foreground: colors.foreground,
                cursor: colors.cursor,
                ansi: colors.ansi
            )
            do {
                let derived = try DesignTokens.derive(
                    from: palette,
                    contrast: appearance.contrast,
                    accent: appearance.accent
                )
                return Tokens(derived, unfocusedDim: CGFloat(terminal.unfocusedDim))
            } catch {
                problems.append("Couldn't derive colours from \(colors.name): \(error)")
                return .fallback
            }
        }

        return Resolved(
            lightTheme: lightTheme,
            darkTheme: darkTheme,
            lightTokens: tokens(lightTheme),
            darkTokens: tokens(darkTheme),
            terminal: TerminalStyle(
                fontFamily: terminal.fontFamily,
                fontSize: terminal.fontSize,
                paddingX: terminal.paddingX,
                paddingY: terminal.paddingY,
                cursor: TerminalStyle.Cursor(terminal.cursor),
                cursorBlinks: terminal.cursorBlinks
            ),
            problems: problems
        )
    }

    nonisolated static let defaultLightTheme = "GitHub Light Default"
    nonisolated static let defaultDarkTheme = "Vesper"
}

extension TerminalStyle.Cursor {
    init(_ shape: AppConfig.Terminal.Cursor) {
        switch shape {
        case .block: self = .block
        case .bar: self = .bar
        case .underline: self = .underline
        }
    }
}
