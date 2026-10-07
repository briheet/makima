import AppKit
import Foundation
import MakimaCore
import MakimaTerminal
import Testing

@testable import MakimaUI

/// Theme resolution: config → Ghostty catalog → Rust token derivation.
@Suite struct AppearanceTests {
    /// The defaults, as `load_config` returns them for a missing file.
    func loaded(_ edit: (inout AppConfig) -> Void = { _ in }, problem: String? = nil) -> AppConfig.Loaded {
        var config = AppConfig.load(path: "/nonexistent/makima/config.toml").config
        edit(&config)
        return AppConfig.Loaded(config: config, path: nil, problem: problem)
    }

    @Test func defaultThemesResolveWithoutProblems() {
        let resolved = AppearanceStore.resolve(loaded())

        #expect(resolved.problems.isEmpty)
        #expect(resolved.lightTheme.name == "GitHub Light Default")
        #expect(resolved.darkTheme.name == "Vesper")
        #expect(resolved.darkTokens.isDark)
        #expect(!resolved.lightTokens.isDark)
    }

    @Test func themeNamesAreCaseInsensitive() {
        let resolved = AppearanceStore.resolve(loaded { $0.appearance.darkTheme = "rose pine" })
        #expect(resolved.darkTheme.name == "Rose Pine")
        #expect(resolved.problems.isEmpty)
    }

    @Test func unknownThemesFallBackAndAreReported() {
        let resolved = AppearanceStore.resolve(loaded { $0.appearance.darkTheme = "Nope" })

        #expect(resolved.darkTheme.name == AppearanceStore.defaultDarkTheme)
        #expect(resolved.problems == ["Unknown theme \"Nope\"; using Vesper."])
    }

    @Test func configProblemsAreCarriedThrough() {
        let resolved = AppearanceStore.resolve(loaded(problem: "config.toml: unknown field `font-sise`"))
        #expect(resolved.problems.first?.contains("font-sise") == true)
    }

    @Test(arguments: ["Catppuccin Latte", "Dracula", "Gruvbox Dark", "Nord", "iTerm2 Solarized Light", "TokyoNight"])
    func catalogThemesDeriveTokens(name: String) {
        let resolved = AppearanceStore.resolve(
            loaded {
                $0.appearance.lightTheme = name
                $0.appearance.darkTheme = name
            }
        )
        #expect(resolved.problems.isEmpty, "\(name): \(resolved.problems)")
    }
}
