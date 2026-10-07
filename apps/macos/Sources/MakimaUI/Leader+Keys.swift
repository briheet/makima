import AppKit
import MakimaCore

/// The configured key from an agent into Makima's keys (`[keys] leader`).
extension AppConfig.Keys.Leader {
    /// How hints, menus and tool windows write it.
    public var label: String {
        switch self {
        case .cmdEsc: "⌘Esc"
        case .ctrlSpace: "^Space"
        case .ctrlO: "^O"
        case .ctrlBracket: "^]"
        case .shiftEsc: "⇧Esc"
        }
    }

    /// Whether `event` is this key, with exactly its modifiers (Caps Lock,
    /// the keypad and Fn don't count).
    func matches(_ event: NSEvent) -> Bool {
        let modifiers = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
            .subtracting([.capsLock, .numericPad, .function])
        return switch self {
        case .cmdEsc: event.keyCode == KeyCode.escape && modifiers == .command
        case .ctrlSpace: event.keyCode == KeyCode.space && modifiers == .control
        case .ctrlO: event.keyCode == KeyCode.o && modifiers == .control
        case .ctrlBracket: event.keyCode == KeyCode.rightBracket && modifiers == .control
        case .shiftEsc: event.keyCode == KeyCode.escape && modifiers == .shift
        }
    }
}
