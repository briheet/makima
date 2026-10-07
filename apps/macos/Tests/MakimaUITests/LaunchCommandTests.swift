import Foundation
import Testing

@testable import MakimaTerminal

@Suite struct LaunchCommandTests {
    @Test func ghosttysCommandIsRead() {
        let config = """
            theme = Vesper
            # command = /bin/bash
            command = /nix/store/abc-nushell/bin/nu
            font-size = 13
            """
        #expect(LaunchCommand.command(inGhosttyConfig: config) == "/nix/store/abc-nushell/bin/nu")
        #expect(LaunchCommand.command(inGhosttyConfig: "theme = x") == nil)
    }

    @Test func theConfiguredShellComesFirst() {
        #expect(LaunchCommand.preferredShell(configured: "nu") == "nu")
        #expect(LaunchCommand(command: nil, shell: "nu").surfaceCommand == "nu")
        #expect(LaunchCommand(command: "lazygit", shell: "nu") == .program("lazygit"))
        let fallback = LaunchCommand.preferredShell(configured: "  ")
        #expect(!fallback.isEmpty, "Ghostty's command or the login shell")
    }
}
