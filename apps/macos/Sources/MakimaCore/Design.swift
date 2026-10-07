internal import MakimaCoreBindings

/// A terminal theme's colours, as `#rrggbb`.
public struct ThemePalette: Sendable, Equatable {
    public var background: String
    public var foreground: String
    public var cursor: String?
    /// ANSI colours 0–15; missing entries are filled from the foreground.
    public var ansi: [String]

    public init(background: String, foreground: String, cursor: String?, ansi: [String]) {
        self.background = background
        self.foreground = foreground
        self.cursor = cursor
        self.ansi = ansi
    }
}

/// The colours AppKit paints around the chrome, derived from a theme by the
/// Rust core (see `docs/design-system.md`).
public struct DesignTokens: Sendable, Equatable {
    public var isDark: Bool
    /// The window and terminal background, `#rrggbb`.
    public var surfaceBase: String

    /// Derives tokens from `palette` at `contrast` (0 to 1), with `accent`
    /// replacing the theme's own.
    /// - Throws: ``CoreFailure/invalidColor(_:)`` for a colour that isn't
    ///   `#rrggbb` or `#rgb`.
    public static func derive(
        from palette: ThemePalette,
        contrast: Float,
        accent: String?
    ) throws(CoreFailure) -> DesignTokens {
        let info: DesignTokensInfo
        do {
            info = try deriveDesignTokens(
                palette: ThemePaletteInfo(
                    background: palette.background,
                    foreground: palette.foreground,
                    cursor: palette.cursor,
                    ansi: palette.ansi
                ),
                contrast: contrast,
                accent: accent
            )
        } catch let error as CoreError {
            throw CoreFailure(error)
        } catch {
            throw .other(String(describing: error))
        }
        return DesignTokens(isDark: info.isDark, surfaceBase: info.surface.base)
    }
}
