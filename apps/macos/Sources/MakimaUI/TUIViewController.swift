import AppKit
import MakimaCore
import MakimaCoreBindings
import MakimaTerminal
import MakimaVoice
import os

/// The whole window: one terminal showing the chrome Rust renders (tabs, box
/// borders, talk line, status line), with each agent's live terminal laid on
/// top at the cell rect the chrome left for it.
public final class TUIViewController: NSViewController {
    private let model: AppModel
    private let appearanceStore: AppearanceStore
    private let tui: TUIController
    private let chrome: ChromeTerminal
    private let paneLayer = PassThroughView()
    /// Above the agents: the chrome's key menu, help, picker and tool
    /// borders, which the agents' terminals would otherwise cover.
    private let overlayLayer = PassThroughView()
    private let overlay: ChromeTerminal
    /// The overlay's last bytes, drawn again when its terminal resizes.
    private var overlayAnsi: String?
    /// Above the panes: tools' floating terminals.
    private let toolLayer = PassThroughView()
    private var panes: [AgentID: PaneView] = [:]
    /// Each work's open tools; hidden ones keep running if the tool keeps.
    private var toolPanes: [TUIController.ToolRef: TerminalPane] = [:]
    private var focusedAgentID: AgentID?
    private var renderLoop: ObservationLoop?
    private var colorLoop: ObservationLoop?
    private var clockTask: Task<Void, Never>?
    private var keyMonitor: Any?
    private let voiceInput = VoiceInput(transcriber: AppleTranscriber())
    private let signposter = OSSignposter(subsystem: "io.makima.app", category: .pointsOfInterest)
    /// Keys with ⌘ or Ctrl, for finding out where a shortcut went:
    /// `log stream --predicate 'subsystem == "io.makima.app"' --level debug`.
    private let keyLogger = Logger(subsystem: "io.makima.app", category: "keys")

    public init(model: AppModel, appearance: AppearanceStore) {
        self.model = model
        appearanceStore = appearance
        tui = TUIController(
            model: model,
            keyStyle: appearance.config.keys.style,
            tools: appearance.config.tools,
            paneKeys: appearance.config.keys.navigate == .off,
            leaderLabel: appearance.config.keys.leader.label
        )
        chrome = ChromeTerminal(controller: appearance.chromeController)
        overlay = ChromeTerminal(controller: appearance.chromeController)
        super.init(nibName: nil, bundle: nil)
        tui.host = self
    }

    @available(*, unavailable)
    required init?(coder _: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    isolated deinit {
        clockTask?.cancel()
        if let keyMonitor { NSEvent.removeMonitor(keyMonitor) }
    }

    // MARK: - Actions for the menu bar

    /// Runs a key-map action, as if typed in Makima's keys.
    public func perform(_ action: KeyAction) {
        tui.perform(action)
    }

    public func addAgent(presetID: String) {
        model.addAgent(presetID: presetID)
    }

    /// Sends a talk-line message as if typed (for `--say` and scripting).
    /// Returns the mistake, if any.
    @discardableResult
    public func say(_ line: String) -> String? {
        tui.say(line)
    }

    /// Opens the tool with id `id` over the work on screen (for `--tool`).
    public func openTool(id: String) {
        guard let index = tui.tools.firstIndex(where: { $0.id == id }) else { return }
        tui.toggleTool(index)
    }

    /// The chrome's on-screen text (a self-test hook for `--dump-chrome`).
    public func chromeText() -> String? {
        chrome.screenText()
    }

    // MARK: - View lifecycle

    override public func loadView() {
        let root = AppearanceTrackingView()
        root.onAppearanceChange = { [weak self] in self?.appearanceStore.systemAppearanceDidChange() }
        root.wantsLayer = true
        view = root
        // Frame-based on purpose: terminal views report sizes of their own,
        // and with constraints the window would grow to fit them.
        root.addSubview(chrome.view)
        root.addSubview(paneLayer)
        root.addSubview(overlayLayer)
        overlayLayer.addSubview(overlay.view)
        overlay.view.isHidden = true
        overlay.onNeedsRedraw = { [weak self] in
            guard let self, let ansi = overlayAnsi else { return }
            overlay.draw(ansi)
        }
        root.addSubview(toolLayer)

        chrome.onResize = { [weak self] grid in self?.tui.resized(to: grid) }
        chrome.onNeedsRedraw = { [weak self] in self?.tui.chromeNeedsRedraw() }
        chrome.onInput = { [weak self] data in self?.tui.feed(data) }
        chrome.view.onClick = { [weak self] point in self?.clicked(point) }
        colorLoop = ObservationLoop { [weak self] in
            guard let self else { return }
            view.layer?.backgroundColor = appearanceStore.tokens.surface.base.cgColor
        }
    }

    override public func viewDidAppear() {
        super.viewDidAppear()
        guard renderLoop == nil else { return }
        renderLoop = ObservationLoop { [weak self] in self?.render() }
        keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self else { return event }
            return handle(event)
        }

        clockTask = Task { [weak self] in
            let formatter = DateFormatter()
            formatter.dateFormat = "HH:mm"
            while !Task.isCancelled {
                self?.tui.setClock(formatter.string(from: .now))
                try? await Task.sleep(for: .seconds(15))
            }
        }
    }

    override public func viewDidLayout() {
        super.viewDidLayout()
        // Everything below the titlebar; the chrome and the pane layer share
        // one frame so chrome cells and pane frames use the same origin.
        let bounds = view.bounds
        let content = NSRect(x: 0, y: 0, width: bounds.width, height: bounds.height - view.safeAreaInsets.top)
        chrome.view.frame = content
        paneLayer.frame = content
        overlayLayer.frame = content
        toolLayer.frame = content
    }

    // MARK: - Rendering

    /// Draws the chrome and lays the panes out. Re-run whenever anything it
    /// reads changes (model, modes, pane titles, grid size).
    private func render() {
        tui.syncWithModel()
        guard let grid = tui.grid else { return }
        let interval = signposter.beginInterval("render", id: signposter.makeSignpostID())
        defer { signposter.endInterval("render", interval) }

        let agents = model.selectedWorkspace?.agents ?? []
        syncPaneViews(agents: agents, allAgents: model.workspaces.flatMap(\.agents))

        let displays = panes.mapValues(\.state.display)
        let frame = TerminalChrome.render(
            tui.chromeState(panes: displays),
            keys: tui.keys,
            screen: tui.screen,
            picker: tui.picker,
            columns: grid.columns,
            rows: grid.rows
        )
        chrome.draw(frame.ansi)
        tui.rendered(panes: frame.panes, tabs: frame.tabs)

        let shown = Set(frame.panes.map(\.id))
        for (id, paneView) in panes {
            paneView.isHidden = !shown.contains(id)
            paneView.setFocused(id == model.selectedAgentID)
        }
        for rect in frame.panes {
            panes[rect.id]?.frame = grid.rect(x: rect.x, y: rect.y, width: rect.width, height: rect.height)
        }
        syncTools(float: frame.float, grid: grid)
        syncOverlay(frame.overlay, grid: grid)
        updateKeyboardFocus()
    }

    /// Creates panes for the work on screen and drops panes whose agent is
    /// gone (ending their process).
    private func syncPaneViews(agents: [Agent], allAgents: [Agent]) {
        let live = Set(allAgents.map(\.id))
        for (id, paneView) in panes where !live.contains(id) {
            paneView.removeFromSuperview()
            panes[id] = nil
        }
        for agent in agents {
            if panes[agent.id] == nil {
                let folder = model.workspace(containing: agent.id)?.folder ?? model.selectedWorkspace?.folder
                let paneView = PaneView(agent: agent, folder: folder, model: model, appearance: appearanceStore)
                paneLayer.addSubview(paneView)
                panes[agent.id] = paneView
            }
        }
    }

    /// Starts the shown tool if it isn't running, places it in the chrome's
    /// float rect, hides the others, and ends hidden ones that don't keep.
    private func syncTools(float: ChromeCellRect?, grid: ChromeGrid) {
        let shown = tui.shownTool
        if let shown, toolPanes[shown] == nil,
            let workspace = model.workspaces.first(where: { $0.id == shown.workspace })
        {
            let tool = tui.tools[shown.tool]
            let pane = TerminalPane(
                command: LaunchCommand(command: tool.command, shell: appearanceStore.shell),
                controller: appearanceStore.terminalController,
                workingDirectory: workspace.folder
            )
            pane.onProgramExit = { [weak self] code in self?.toolEnded(shown, code: code) }
            pane.onClose = { [weak self] in self?.toolEnded(shown, code: nil) }
            toolLayer.addSubview(pane.view)
            toolPanes[shown] = pane
        }
        let open = Set(model.workspaces.map(\.id))
        for (ref, pane) in toolPanes {
            if ref == shown, let float {
                pane.view.isHidden = false
                pane.view.frame = grid.rect(x: float.x, y: float.y, width: float.width, height: float.height)
                pane.fitToSize()
            } else if !tui.tools[ref.tool].keeps || !open.contains(ref.workspace) {
                toolEnded(ref, code: nil)
            } else {
                pane.view.isHidden = true
            }
        }
    }

    /// Shows the part of the chrome that must sit above the agents, in its
    /// own terminal the size of its rect.
    private func syncOverlay(_ part: ChromeOverlay?, grid: ChromeGrid) {
        guard let part else {
            overlay.view.isHidden = true
            overlayAnsi = nil
            return
        }
        let rect = grid.rect(x: part.rect.x, y: part.rect.y, width: part.rect.width, height: part.rect.height)
        if overlay.view.frame != rect { overlay.view.frame = rect }
        overlay.view.isHidden = false
        overlayAnsi = part.ansi
        overlay.draw(part.ansi)
    }

    /// A tool's program ended (or it was put away for good).
    private func toolEnded(_ ref: TUIController.ToolRef, code: Int32?) {
        toolPanes.removeValue(forKey: ref)?.view.removeFromSuperview()
        tui.toolExited(ref, code: code)
    }

    /// The chrome takes the keyboard while typing, a tool while it's showing;
    /// otherwise the selected pane.
    private func updateKeyboardFocus() {
        guard let window = view.window else { return }
        if tui.mode == .tool, let shown = tui.shownTool, let pane = toolPanes[shown] {
            chrome.view.acceptsKeyboard = false
            if !pane.isFocused { pane.focus() }
            focusedAgentID = nil
            return
        }
        chrome.view.acceptsKeyboard = tui.isTyping
        if tui.isTyping {
            if window.firstResponder !== chrome.view { window.makeFirstResponder(chrome.view) }
            focusedAgentID = nil
            return
        }
        guard let selected = model.selectedAgentID, let paneView = panes[selected], !paneView.isHidden else { return }
        if focusedAgentID != selected || window.firstResponder === chrome.view {
            focusedAgentID = selected
            view.layoutSubtreeIfNeeded()
            paneView.pane.focus()
        }
    }

    // MARK: - Input

    /// The leader key, Makima's keys and confirmations. Returns `nil`
    /// for keys Makima consumed.
    private func handle(_ event: NSEvent) -> NSEvent? {
        let modifiers = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        let isToggle = appearanceStore.config.keys.leader.matches(event)
        if !modifiers.isDisjoint(with: [.command, .control]) {
            keyLogger.debug(
                "key \(event.keyCode) modifiers \(modifiers.rawValue) leader \(isToggle) mode \(String(describing: self.tui.mode), privacy: .public)"
            )
        }
        switch tui.mode {
        case .keys:
            if isToggle {
                tui.toggleKeys()
            } else if let token = Self.keyToken(for: event, modifiers: modifiers) {
                tui.press(token)
            }
            return nil
        case .confirm:
            tui.answer(event.charactersIgnoringModifiers?.lowercased() == "y")
            return nil
        case .talk, .command, .newWork, .picker:
            // Ctrl+Space still works (e.g. to speak); everything else goes
            // to the chrome terminal, which encodes it for Rust.
            if isToggle {
                tui.toggleKeys()
                return nil
            }
            return event
        case .tool:
            // Everything goes to the tool except Ctrl+Space.
            if isToggle {
                tui.toggleKeys()
                return nil
            }
            return event
        case .pane:
            if isToggle {
                tui.toggleKeys()
                return nil
            }
            if let direction = navigation(for: event, modifiers: modifiers) {
                tui.perform(.focus(direction: direction))
                return nil
            }
            return event
        }
    }

    /// Ctrl (or Alt) + h/j/k/l, per `[keys] navigate`: a move between
    /// panes while typing in one.
    private func navigation(for event: NSEvent, modifiers: NSEvent.ModifierFlags) -> FocusDirection? {
        let required: NSEvent.ModifierFlags? =
            switch appearanceStore.config.keys.navigate {
            case .ctrl: .control
            case .alt: .option
            case .off: nil
            }
        guard let required, modifiers == required else { return nil }
        // Alt changes the character (⌥h is ˙), so read the key's own letter.
        switch event.keyCode {
        case KeyCode.h: return .left
        case KeyCode.j: return .down
        case KeyCode.k: return .up
        case KeyCode.l: return .right
        default: return nil
        }
    }

    /// A key named like Helix's key map: `h`, `G`, `space`, `enter`,
    /// `esc`, `left`, `C-w`. `nil` for keys with no name (e.g. ⌘-chords).
    private static func keyToken(for event: NSEvent, modifiers: NSEvent.ModifierFlags) -> String? {
        if modifiers.contains(.command) { return nil }
        switch event.keyCode {
        case KeyCode.escape: return "esc"
        case KeyCode.returnKey, KeyCode.keypadEnter: return "enter"
        case KeyCode.tab: return "tab"
        case KeyCode.left: return "left"
        case KeyCode.down: return "down"
        case KeyCode.up: return "up"
        case KeyCode.right: return "right"
        case KeyCode.space: return modifiers.contains(.control) ? "C-space" : "space"
        default: break
        }
        if modifiers.contains(.control) {
            return event.charactersIgnoringModifiers.flatMap { $0.isEmpty ? nil : "C-\($0.lowercased())" }
        }
        return event.characters.flatMap { $0.isEmpty ? nil : $0 }
    }

    /// Clicking a tab switches work.
    private func clicked(_ point: NSPoint) {
        guard let grid = tui.grid else { return }
        let (column, row) = grid.cell(at: point)
        guard row == 0, let hit = tui.lastTabs.first(where: { column >= $0.start && column < $0.end }) else { return }
        model.selectWorkspace(number: Int(hit.index) + 1)
    }
}

extension TUIViewController: TUIHost {
    func send(_ text: String, to agent: AgentID) {
        panes[agent]?.pane.send(text)
    }

    func type(_ text: String, into agent: AgentID) {
        panes[agent]?.pane.type(text)
    }

    func sendLeaderKeyToFocusedPane() {
        // Only Ctrl+Space is a key the pane could want; ⌘ never reaches it.
        guard appearanceStore.config.keys.leader == .ctrlSpace, let selected = model.selectedAgentID else { return }
        panes[selected]?.pane.sendLeaderKey()
    }

    func startRecording() async throws {
        voiceInput.transcriber = try await Self.transcriber(for: appearanceStore.config.voice)
        let directory =
            model.recordingsDirectory.map(URL.init(fileURLWithPath:))
            ?? FileManager.default.temporaryDirectory.appendingPathComponent("makima-recordings")
        try await voiceInput.start(in: directory)
    }

    /// The transcriber the config asks for, checked before recording so a
    /// missing key is reported straight away. The key may come from the
    /// Keychain, so this runs off the main thread.
    @concurrent
    private static func transcriber(for settings: AppConfig.Voice) async throws -> any Transcriber {
        switch settings.provider {
        case .off:
            throw VoiceError.unavailable("Voice is off; set [voice] provider in the config.")
        case .apple:
            return AppleTranscriber()
        case .openai:
            guard let baseURL = URL(string: settings.baseURL) else {
                throw VoiceError.unavailable("[voice] base-url isn't a URL: \(settings.baseURL)")
            }
            guard let key = APIKey.find(environmentVariable: settings.apiKeyVariable, account: "openai") else {
                throw VoiceError.unavailable(
                    "No API key: set \(settings.apiKeyVariable), or run "
                        + "security add-generic-password -s makima -a openai -w")
            }
            return APITranscriber(baseURL: baseURL, model: settings.model, apiKey: key)
        }
    }

    func finishRecording() async throws -> VoiceClip {
        try await voiceInput.finish(language: appearanceStore.config.voice.language)
    }
}

/// macOS virtual key codes for the keys the TUI handles itself. Constants,
/// so usable from any isolation.
nonisolated enum KeyCode {
    static let h: UInt16 = 4
    static let j: UInt16 = 38
    static let k: UInt16 = 40
    static let l: UInt16 = 37
    static let o: UInt16 = 31
    static let rightBracket: UInt16 = 30
    static let space: UInt16 = 49
    static let returnKey: UInt16 = 36
    static let tab: UInt16 = 48
    static let escape: UInt16 = 53
    static let keypadEnter: UInt16 = 76
    static let left: UInt16 = 123
    static let right: UInt16 = 124
    static let down: UInt16 = 125
    static let up: UInt16 = 126
}

/// Calls `onAppearanceChange` when the effective appearance (light/dark)
/// changes; AppKit delivers it on the main actor.
private final class AppearanceTrackingView: NSView {
    var onAppearanceChange: (() -> Void)?

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        onAppearanceChange?()
    }
}
