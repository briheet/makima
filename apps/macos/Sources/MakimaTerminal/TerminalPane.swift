import AppKit
import GhosttyTerminal

/// One live terminal surface backed by a real process.
///
/// The pane owns its `AppTerminalView` and reports what happens to the
/// process through closures; it never mirrors terminal contents, which stay
/// in libghostty.
public final class TerminalPane {
    public let view: AppTerminalView

    /// The title the running program set (shell integration sets it to the
    /// current command).
    public var onTitleChange: ((String) -> Void)?
    /// The shell's working directory (OSC 7).
    public var onWorkingDirectoryChange: ((String) -> Void)?
    public var onFocusChange: ((Bool) -> Void)?
    /// The program asked for attention: a desktop notification or the bell.
    public var onAttention: ((String) -> Void)?
    /// A `.program` exited with this code. Its output stays on screen until
    /// the user dismisses it, which then closes the pane.
    public var onProgramExit: ((Int32) -> Void)?
    /// The surface closed; `view` shows nothing live after this.
    public var onClose: (() -> Void)?

    public private(set) var isClosed = false

    private let delegateProxy = DelegateProxy()

    /// - Parameters:
    ///   - command: What to run.
    ///   - controller: The shared libghostty controller (theme, font, keys).
    ///   - workingDirectory: Starting directory; `nil` uses the home directory.
    ///   - environment: Extra variables, e.g. to tag the process with its agent.
    public init(
        command: LaunchCommand,
        controller: TerminalController,
        workingDirectory: String? = nil,
        environment: [String: String] = [:]
    ) {
        view = AppTerminalView(frame: .zero)
        view.controller = controller
        view.configuration = TerminalSurfaceOptions(
            backend: .exec,
            workingDirectory: workingDirectory ?? FileManager.default.homeDirectoryForCurrentUser.path,
            envVars: environment,
            command: command.surfaceCommand,
            waitAfterCommand: command.waitsAfterExit
        )
        delegateProxy.pane = self
        view.delegate = delegateProxy
    }

    public func focus() {
        view.window?.makeFirstResponder(view)
    }

    public var isFocused: Bool {
        view.window?.firstResponder === view
    }

    public func fitToSize() {
        view.fitToSize()
    }

    /// Types `message` into the program as a paste, then presses Enter — how
    /// the talk line delivers to an agent (like tmux `send-keys`).
    public func send(_ message: String) {
        _ = view.paste(text: message)
        _ = view.sendKey(.enter)
    }

    /// Types `text` into the program as a paste, without pressing Enter, so
    /// it can be read and edited first (a voice note).
    public func type(_ text: String) {
        _ = view.paste(text: text)
    }

    /// Sends a literal Ctrl+Space (the leader key pressed twice).
    public func sendLeaderKey() {
        _ = view.sendKey(.space, modifiers: .ctrl)
    }

    fileprivate func handleNotification(title: String, body: String) {
        if title == LaunchCommand.exitNotificationTitle {
            onProgramExit?(Int32(body) ?? 1)
        } else {
            onAttention?(body.isEmpty ? title : body)
        }
    }

    fileprivate func handleClose() {
        guard !isClosed else { return }
        isClosed = true
        onClose?()
    }
}

/// `AppTerminalView.delegate` is weak, so the pane holds this proxy strongly
/// and the proxy points back weakly — no retain cycle.
private final class DelegateProxy:
    TerminalSurfaceTitleDelegate,
    TerminalSurfacePwdDelegate,
    TerminalSurfaceFocusDelegate,
    TerminalSurfaceBellDelegate,
    TerminalSurfaceDesktopNotificationDelegate,
    TerminalSurfaceCloseDelegate
{
    weak var pane: TerminalPane?

    func terminalDidChangeTitle(_ title: String) {
        pane?.onTitleChange?(title)
    }

    func terminalDidChangeWorkingDirectory(_ path: String) {
        pane?.onWorkingDirectoryChange?(path)
    }

    func terminalDidChangeFocus(_ focused: Bool) {
        pane?.onFocusChange?(focused)
    }

    func terminalDidRingBell() {
        pane?.onAttention?("Bell")
    }

    func terminalDidRequestDesktopNotification(title: String, body: String) {
        pane?.handleNotification(title: title, body: body)
    }

    func terminalDidClose(processAlive _: Bool) {
        pane?.handleClose()
    }
}
