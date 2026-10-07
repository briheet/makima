import Darwin
import Foundation

/// What a terminal pane runs. Plain values and file reads, so usable from
/// any isolation.
public nonisolated enum LaunchCommand: Sendable, Equatable {
    /// A shell command line, e.g. `nu` or `/bin/zsh -l`, run as-is.
    case shell(String)
    /// A program such as `claude`, run through the user's login shell so it
    /// sees the same `PATH` as their own terminal.
    case program(String)

    /// `command`, or `shell` when there's no command (a shell pane or the
    /// shell tool).
    public init(command: String?, shell: String) {
        self = command.map(Self.program) ?? .shell(shell)
    }

    /// The shell to open: the config's `[terminal] shell`, else Ghostty's
    /// own `command` (the shell the user's terminal opens), else the login
    /// shell from the user database. Never `$SHELL`, which tools like
    /// `nix develop` change.
    public static func preferredShell(configured: String?) -> String {
        if let configured, !configured.trimmingCharacters(in: .whitespaces).isEmpty { return configured }
        if let ghostty = ghosttyCommand() { return ghostty }
        return "\(shellQuoted(userLoginShell)) -l"
    }

    /// `command = …` from the user's Ghostty config, if set.
    static func ghosttyCommand(home: URL = FileManager.default.homeDirectoryForCurrentUser) -> String? {
        let configHome =
            ProcessInfo.processInfo.environment["XDG_CONFIG_HOME"].map { URL(fileURLWithPath: $0) }
            ?? home.appendingPathComponent(".config")
        let files = [
            configHome.appendingPathComponent("ghostty/config"),
            home.appendingPathComponent("Library/Application Support/com.mitchellh.ghostty/config"),
        ]
        for file in files {
            guard let text = try? String(contentsOf: file, encoding: .utf8) else { continue }
            if let command = Self.command(inGhosttyConfig: text) { return command }
        }
        return nil
    }

    /// The last `command = …` line of a Ghostty config.
    static func command(inGhosttyConfig text: String) -> String? {
        text.split(separator: "\n").reversed().lazy.compactMap { line -> String? in
            let parts = line.split(separator: "=", maxSplits: 1)
            guard parts.count == 2, parts[0].trimmingCharacters(in: .whitespaces) == "command" else { return nil }
            let value = parts[1].trimmingCharacters(in: .whitespaces).trimmingCharacters(
                in: CharacterSet(charactersIn: "\""))
            return value.isEmpty ? nil : value
        }.first
    }

    /// The marker title in the OSC 777 notification the program wrapper
    /// prints on exit; the body is the exit code.
    static let exitNotificationTitle = "makima-exit"

    /// The command for the Ghostty surface.
    var surfaceCommand: String {
        switch self {
        case .shell(let shell):
            return shell
        case .program(let program):
            let shell = Self.userLoginShell
            // Run the program in an interactive login shell, then report its
            // exit status through an OSC 777 notification the pane can parse.
            let script = """
                \(shellQuoted(shell)) -l -i -c \(shellQuoted("exec \(program)")); \
                code=$?; \
                printf '\\033]777;notify;\(Self.exitNotificationTitle);%d\\007' "$code"; \
                exit "$code"
                """
            return "/bin/sh -c \(shellQuoted(script))"
        }
    }

    /// Programs keep their output on screen after exiting; shells close.
    var waitsAfterExit: Bool {
        if case .program = self { true } else { false }
    }

    /// The login shell from the user database, which GUI apps can't rely on
    /// `$SHELL` for.
    static var userLoginShell: String {
        guard let entry = getpwuid(getuid()), let shell = entry.pointee.pw_shell else { return "/bin/zsh" }
        return String(cString: shell)
    }
}

/// `value` as a single-quoted POSIX shell word.
nonisolated func shellQuoted(_ value: String) -> String {
    "'" + value.replacingOccurrences(of: "'", with: #"'\''"#) + "'"
}
