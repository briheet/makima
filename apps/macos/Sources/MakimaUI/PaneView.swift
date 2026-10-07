import AppKit
import MakimaCore
import MakimaTerminal
import Observation

/// What a pane's terminal reports beyond the core's agent record. Ephemeral:
/// none of it is persisted or sent to the core.
@Observable
final class PaneState {
    var title = ""
    /// The surface closed and shows nothing live; restart or close the agent.
    var isClosed = false

    var display: PaneDisplay { PaneDisplay(title: title, isClosed: isClosed) }
}

/// One agent's live terminal, placed by the chrome inside its box border.
/// Unfocused panes get a veil of the background colour, like Ghostty's
/// unfocused splits.
final class PaneView: NSView {
    let agentID: AgentID
    let state = PaneState()
    let pane: TerminalPane
    private let model: AppModel
    private let appearanceStore: AppearanceStore
    private let veil = PassThroughView()
    private var isFocused = false
    private var colorLoop: ObservationLoop?

    /// - Parameter folder: Where the agent works; home when `nil`.
    init(agent: Agent, folder: String?, model: AppModel, appearance: AppearanceStore) {
        agentID = agent.id
        self.model = model
        appearanceStore = appearance
        pane = Self.makePane(for: agent, folder: folder, appearance: appearance)
        super.init(frame: .zero)
        attach(pane)
        veil.wantsLayer = true
        addSubview(veil)
        colorLoop = ObservationLoop { [weak self] in self?.applyVeil() }
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func setFocused(_ focused: Bool) {
        guard focused != isFocused else { return }
        isFocused = focused
        applyVeil()
    }

    override func layout() {
        super.layout()
        pane.view.frame = bounds
        veil.frame = bounds
        pane.fitToSize()
    }

    private func applyVeil() {
        let tokens = appearanceStore.tokens
        veil.layer?.backgroundColor = tokens.surface.base.withAlphaComponent(tokens.unfocusedDim).cgColor
        veil.isHidden = isFocused || tokens.unfocusedDim == 0
    }

    private static func makePane(for agent: Agent, folder: String?, appearance: AppearanceStore) -> TerminalPane {
        TerminalPane(
            command: LaunchCommand(command: agent.command, shell: appearance.shell),
            controller: appearance.terminalController,
            workingDirectory: folder,
            environment: ["MAKIMA_AGENT_ID": String(describing: agent.id)]
        )
    }

    private func attach(_ pane: TerminalPane) {
        addSubview(pane.view, positioned: .below, relativeTo: veil)
        let agentID = agentID
        pane.onTitleChange = { [weak self] in self?.state.title = $0 }
        pane.onFocusChange = { [weak self] focused in
            if focused { self?.model.select(agentID) }
        }
        pane.onAttention = { [weak self] message in
            self?.model.agentNeedsAttention(id: agentID, message: message)
        }
        pane.onProgramExit = { [weak self] code in
            self?.model.agentExited(id: agentID, code: code)
        }
        pane.onClose = { [weak self] in
            self?.model.agentExited(id: agentID, code: nil)
            self?.state.isClosed = true
        }
        model.agentStarted(id: agentID)
    }
}

/// A view that draws but never takes clicks, so they reach what's beneath.
final class PassThroughView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? {
        let hit = super.hitTest(point)
        return hit === self ? nil : hit
    }

    override var isFlipped: Bool { true }
}
