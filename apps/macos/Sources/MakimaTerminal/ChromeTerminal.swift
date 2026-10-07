import AppKit
import GhosttyTerminal

/// The chrome's terminal grid, in points.
public struct ChromeGrid: Equatable, Sendable {
    public let columns: UInt16
    public let rows: UInt16
    public let cellWidth: CGFloat
    public let cellHeight: CGFloat

    /// The rect, in the chrome view's flipped coordinates, of a cell range.
    public func rect(x: UInt16, y: UInt16, width: UInt16, height: UInt16) -> NSRect {
        NSRect(
            x: CGFloat(x) * cellWidth,
            y: CGFloat(y) * cellHeight,
            width: CGFloat(width) * cellWidth,
            height: CGFloat(height) * cellHeight
        )
    }

    /// The cell under `point` (flipped coordinates).
    public func cell(at point: NSPoint) -> (column: Int, row: Int) {
        (Int(point.x / cellWidth), Int(point.y / cellHeight))
    }
}

/// A terminal that shows Makima's chrome: no process behind it, just the
/// bytes Makima renders (`draw(_:)`). When it has keyboard focus, the bytes
/// Ghostty encodes for each key arrive in `onInput`, exactly as a TUI would
/// read them from stdin.
public final class ChromeTerminal {
    public let view: ChromeTerminalView
    /// Raw input bytes typed while the chrome has focus.
    public var onInput: ((Data) -> Void)?
    /// The grid changed size.
    public var onResize: ((ChromeGrid) -> Void)?
    /// The terminal resized its screen, which clears it: draw again.
    public var onNeedsRedraw: (() -> Void)?

    private let session: InMemoryTerminalSession
    private let delegateProxy = ChromeDelegateProxy()
    private var inputTask: Task<Void, Never>?
    private var redrawTask: Task<Void, Never>?

    public init(controller: TerminalController) {
        // The session calls back from its own thread; the streams hop input
        // and resizes onto the main actor in tasks this object owns.
        let (input, inputContinuation) = AsyncStream.makeStream(of: Data.self)
        let (resizes, resizeContinuation) = AsyncStream.makeStream(of: Void.self, bufferingPolicy: .bufferingNewest(1))
        session = InMemoryTerminalSession(
            write: { inputContinuation.yield($0) },
            resize: { _ in resizeContinuation.yield() }
        )
        view = ChromeTerminalView(frame: .zero)
        // The surface reports a size of its own; the window, not the
        // chrome, decides how big the grid is.
        for orientation in [NSLayoutConstraint.Orientation.horizontal, .vertical] {
            view.setContentHuggingPriority(.fittingSizeCompression, for: orientation)
            view.setContentCompressionResistancePriority(.fittingSizeCompression, for: orientation)
        }
        view.controller = controller
        view.configuration = TerminalSurfaceOptions(backend: .inMemory(session))
        // Grid metrics (with cell sizes) come from the surface itself.
        delegateProxy.onResize = { [weak self] metrics in self?.resized(metrics) }
        view.delegate = delegateProxy

        inputTask = Task { [weak self] in
            for await data in input { self?.onInput?(data) }
        }
        redrawTask = Task { [weak self] in
            for await _ in resizes { self?.onNeedsRedraw?() }
        }
    }

    deinit {
        inputTask?.cancel()
        redrawTask?.cancel()
    }

    /// The text currently on screen, for self-tests and debugging.
    public func screenText() -> String? {
        session.readViewportText()
    }

    /// Draws a frame (ANSI bytes from the renderer).
    public func draw(_ ansi: String) {
        session.receive(ansi)
    }

    private func resized(_ metrics: TerminalGridMetrics) {
        guard metrics.columns > 0, metrics.rows > 0, metrics.cellWidthPixels > 0, metrics.widthPixels > 0,
            view.bounds.width > 0
        else { return }
        // Ghostty reports pixels at whatever scale the surface has right now
        // (1× before the window reaches a Retina screen, 2× after), so derive
        // points-per-pixel from the surface itself rather than the screen.
        let scale = CGFloat(metrics.widthPixels) / view.bounds.width
        onResize?(
            ChromeGrid(
                columns: metrics.columns,
                rows: metrics.rows,
                cellWidth: CGFloat(metrics.cellWidthPixels) / scale,
                cellHeight: CGFloat(metrics.cellHeightPixels) / scale
            )
        )
    }
}

/// The chrome's view: takes keyboard focus only while Makima wants typing
/// (the talk line), and reports clicks for the tab line.
public final class ChromeTerminalView: AppTerminalView {
    public var acceptsKeyboard = false
    /// A click, in the view's top-left-origin coordinates.
    public var onClick: ((NSPoint) -> Void)?

    override public var acceptsFirstResponder: Bool { acceptsKeyboard }

    override public func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        onClick?(isFlipped ? point : NSPoint(x: point.x, y: bounds.height - point.y))
        if acceptsKeyboard { super.mouseDown(with: event) }
    }
}

/// `AppTerminalView.delegate` is weak; the chrome holds this strongly.
private final class ChromeDelegateProxy: TerminalSurfaceGridResizeDelegate {
    var onResize: ((TerminalGridMetrics) -> Void)?

    func terminalDidResize(_ size: TerminalGridMetrics) {
        onResize?(size)
    }
}
