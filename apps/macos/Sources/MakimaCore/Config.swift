internal import MakimaCoreBindings

/// The user's settings from `config.toml`, parsed and validated by the Rust
/// core. Built-ins are applied, so every field has a value.
public struct AppConfig: Sendable, Equatable {
    public struct Appearance: Sendable, Equatable {
        public enum Mode: Sendable {
            case system, light, dark
        }

        public var mode: Mode
        public var lightTheme: String
        public var darkTheme: String
        /// `#rrggbb`, replacing the theme's accent.
        public var accent: String?
        /// 0 is softest, 1 strongest.
        public var contrast: Float
    }

    public struct Terminal: Sendable, Equatable {
        public enum Cursor: Sendable {
            case block, bar, underline
        }

        public var fontFamily: String?
        public var fontSize: Float
        public var paddingX: Int
        public var paddingY: Int
        /// The veil over unfocused panes, 0 to 0.6.
        public var unfocusedDim: Float
        public var cursor: Cursor
        public var cursorBlinks: Bool
        /// The shell for shell panes and tools; `nil` lets the app choose.
        public var shell: String?
    }

    public struct Keys: Sendable, Equatable {
        public enum Style: Sendable {
            /// Modal: the leader enters NOR, `i` goes back to the agent.
            case helix
            /// The leader, then one key.
            case tmux
        }

        /// The key from an agent into Makima's keys.
        public enum Leader: Sendable {
            case cmdEsc, ctrlSpace, ctrlO, ctrlBracket, shiftEsc
        }

        /// The modifier that, with `h j k l`, moves between agents while
        /// typing; `off` moves them to Makima's keys.
        public enum Navigate: Sendable {
            case ctrl, alt, off
        }

        public var style: Style
        public var leader: Leader
        public var navigate: Navigate
    }

    /// A program opened in a floating window over a work, in its folder.
    public struct Tool: Identifiable, Sendable, Equatable {
        public var id: String
        public var label: String
        /// `nil` runs the login shell.
        public var command: String?
        /// One character after `space` (or the prefix), or `nil` for the
        /// tools menu only.
        public var key: String?
        /// Share of the screen, 30 to 100.
        public var percent: UInt8
        /// Keeps running while hidden.
        public var keeps: Bool

        public init(id: String, label: String, command: String?, key: String?, percent: UInt8, keeps: Bool) {
            self.id = id
            self.label = label
            self.command = command
            self.key = key
            self.percent = percent
            self.keeps = keeps
        }
    }

    public struct Voice: Sendable, Equatable {
        public enum Provider: Sendable {
            /// macOS 26's on-device `SpeechAnalyzer`.
            case apple
            /// An OpenAI-compatible transcription API.
            case openai
            case off
        }

        public var provider: Provider
        /// BCP 47, e.g. `en-US`; the system language when `nil`.
        public var language: String?
        /// For `openai`: the model, the API's base URL, and the environment
        /// variable holding the key.
        public var model: String
        public var baseURL: String
        public var apiKeyVariable: String
    }

    /// A config file read from disk, with anything wrong with it.
    public struct Loaded: Sendable, Equatable {
        /// The defaults stand in for the file when `problem` is set.
        public var config: AppConfig
        /// The file that was (or would be) read.
        public var path: String?
        /// Why the file couldn't be used. A missing file isn't a problem.
        public var problem: String?

        public init(config: AppConfig, path: String?, problem: String?) {
            self.config = config
            self.path = path
            self.problem = problem
        }
    }

    public var appearance: Appearance
    public var terminal: Terminal
    public var keys: Keys
    /// Built-in tools with `[[tools]]` applied, in menu order.
    public var tools: [Tool]
    public var voice: Voice
    /// Built-in presets with `[[agents]]` applied.
    public var presets: [AgentPreset]

    /// Reads `path`, or the default location when `nil`. Never fails:
    /// problems are reported in ``Loaded/problem`` and the defaults used.
    public static func load(path: String?) -> Loaded {
        let loaded = loadConfig(path: path)
        return Loaded(config: AppConfig(loaded.config), path: loaded.path, problem: loaded.problem)
    }

    /// A commented config with every default, for creating the file.
    public static var defaultText: String {
        defaultConfigText()
    }
}

// MARK: - Conversions from the generated FFI types

extension AppConfig {
    init(_ info: ConfigInfo) {
        self.init(
            appearance: Appearance(info.appearance),
            terminal: Terminal(info.terminal),
            keys: Keys(
                style: info.keyStyle == .tmux ? .tmux : .helix,
                leader: Keys.Leader(info.leader),
                navigate: Keys.Navigate(info.navigate)
            ),
            tools: info.tools.map(Tool.init),
            voice: Voice(info.voice),
            presets: info.presets.map(AgentPreset.init)
        )
    }
}

extension AppConfig.Appearance {
    init(_ info: AppearanceSettings) {
        let mode: Mode =
            switch info.mode {
            case .system: .system
            case .light: .light
            case .dark: .dark
            }
        self.init(
            mode: mode,
            lightTheme: info.lightTheme,
            darkTheme: info.darkTheme,
            accent: info.accent,
            contrast: info.contrast
        )
    }
}

extension AppConfig.Terminal {
    init(_ info: TerminalSettings) {
        let cursor: Cursor =
            switch info.cursorStyle {
            case .block: .block
            case .bar: .bar
            case .underline: .underline
            }
        self.init(
            fontFamily: info.fontFamily,
            fontSize: info.fontSize,
            paddingX: Int(info.paddingX),
            paddingY: Int(info.paddingY),
            unfocusedDim: info.unfocusedDim,
            cursor: cursor,
            cursorBlinks: info.cursorBlink,
            shell: info.shell
        )
    }
}

extension AppConfig.Keys.Leader {
    init(_ key: LeaderKey) {
        switch key {
        case .cmdEsc: self = .cmdEsc
        case .ctrlSpace: self = .ctrlSpace
        case .ctrlO: self = .ctrlO
        case .ctrlBracket: self = .ctrlBracket
        case .shiftEsc: self = .shiftEsc
        }
    }
}

extension AppConfig.Keys.Navigate {
    init(_ keys: NavigateKeys) {
        switch keys {
        case .ctrl: self = .ctrl
        case .alt: self = .alt
        case .off: self = .off
        }
    }
}

extension AppConfig.Tool {
    init(_ info: ToolInfo) {
        self.init(
            id: info.id,
            label: info.label,
            command: info.command,
            key: info.key,
            percent: info.percent,
            keeps: info.keep
        )
    }
}

extension AppConfig.Voice {
    init(_ info: VoiceSettings) {
        let provider: Provider =
            switch info.provider {
            case .apple: .apple
            case .openai: .openai
            case .off: .off
            }
        self.init(
            provider: provider,
            language: info.language,
            model: info.model,
            baseURL: info.baseUrl,
            apiKeyVariable: info.apiKeyEnv
        )
    }
}
