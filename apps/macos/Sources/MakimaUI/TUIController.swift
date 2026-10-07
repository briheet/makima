import Foundation
import MakimaCore
import MakimaCoreBindings
import MakimaTerminal
import Observation

/// What the TUI needs from the window hosting it: delivering text to an
/// agent's terminal, passing the leader key through, and capturing voice. A
/// protocol so tests can stand in without terminals or a microphone.
protocol TUIHost: AnyObject {
    /// Pastes `text` into `agent`'s terminal and presses Enter.
    func send(_ text: String, to agent: AgentID)
    /// Pastes `text` into `agent`'s terminal without pressing Enter.
    func type(_ text: String, into agent: AgentID)
    /// Sends a literal Ctrl+Space to the focused pane.
    func sendLeaderKeyToFocusedPane()
    /// Starts recording the microphone.
    func startRecording() async throws
    /// Stops recording and transcribes it.
    func finishRecording() async throws -> VoiceClip
}

/// What a pane shows beyond the agent record: its terminal's title, and
/// whether the terminal has closed.
struct PaneDisplay: Equatable {
    var title = ""
    var isClosed = false
}

/// The TUI's input state machine. Keys either go to the focused pane or
/// are Makima commands, in one of two styles (`[keys] style`):
///
/// - **Helix:** Ctrl+Space enters NOR, which stays until `i`: `h j k l`
///   move between panes, `g n`/`g p` between works, `space b` picks one.
/// - **tmux:** Ctrl+Space, then one key, then back to the pane.
///
/// Around that: the new-work screen, a work picker, a talk line, a
/// `makima>` prompt, y/n confirmations and push-to-talk. It turns
/// keys into model intents and builds the `ChromeState` Rust draws.
@Observable
final class TUIController {
    enum Mode: Equatable {
        /// Keys go to the focused pane (Helix's INS).
        case pane
        /// Keys are Makima commands (Helix's NOR; tmux's prefix).
        case keys
        /// The welcome screen, template picker and form.
        case newWork
        /// Picking a work from a fuzzy list.
        case picker
        /// Typing a message for `target` (overridable with `@…`).
        case talk(TalkTarget)
        /// Typing a `makima>` command.
        case command
        /// Waiting for y/n.
        case confirm(Confirmation)
        /// Keys go to the tool in the floating window.
        case tool
    }

    /// One work's copy of one tool: each work has its own, in its folder.
    struct ToolRef: Hashable {
        let workspace: WorkspaceID
        /// Index into ``tools``.
        let tool: Int
    }

    struct Confirmation: Equatable {
        let question: String
        let agent: AgentID
    }

    private(set) var mode: Mode = .pane
    private(set) var showHelp = false
    /// A one-line message (an input mistake) for the talk line.
    private(set) var notice: String?
    /// The talk line as last edited (mirrored from the Rust editor).
    private(set) var lineText = ""
    private(set) var lineCursor: UInt32 = 0
    /// The new-work screen while it's showing.
    private(set) var screen: NewWorkScreen?
    /// The work picker while it's showing.
    private(set) var picker: Picker?
    /// The screen, picker and key map change inside Rust; bumped so the
    /// chrome re-renders.
    private(set) var revision = 0
    /// The chrome's grid; `nil` until the chrome terminal reports a size.
    private(set) var grid: ChromeGrid?
    /// `HH:mm`, for the status line.
    private(set) var clock = ""
    /// Bumped when the chrome terminal cleared itself and must be redrawn.
    private(set) var redraws = 0

    @ObservationIgnored let model: AppModel
    @ObservationIgnored let keys: KeymapEngine
    @ObservationIgnored weak var host: (any TUIHost)?
    /// Pane rects and tab hits from the last rendered frame.
    @ObservationIgnored private(set) var lastPanes: [ChromePaneRect] = []
    @ObservationIgnored private(set) var lastTabs: [ChromeTabHit] = []
    @ObservationIgnored private let editor = TalkEditor()
    /// The mode to return to after an action chosen while typing.
    @ObservationIgnored private var resumeMode: Mode?
    /// A spoken message waiting in the talk line, saved with it when sent.
    @ObservationIgnored private var pendingVoice: VoiceClip?
    @ObservationIgnored private let pushToTalk = PushToTalk()
    /// The screen is showing only because there was no work; it gives way
    /// as soon as work exists (e.g. started from a script).
    @ObservationIgnored private var screenIsEmptyState = false

    /// The tool in the floating window, if one is showing.
    private(set) var shownTool: ToolRef?
    /// The floating window fills the body.
    private(set) var toolZoomed = false
    /// Tools that can open, in menu order (`Action.tool(index:)` indexes it).
    @ObservationIgnored let tools: [AppConfig.Tool]
    /// How the key into Makima's keys is written in hints.
    @ObservationIgnored let leaderLabel: String

    /// - Parameter paneKeys: `h j k l` move between agents in Makima's keys,
    ///   for when Ctrl+h/j/k/l is off.
    ///   - leaderLabel: How the key into Makima's keys is written, e.g. `^Space`.
    init(
        model: AppModel,
        keyStyle: AppConfig.Keys.Style = .helix,
        tools: [AppConfig.Tool] = [],
        paneKeys: Bool = false,
        leaderLabel: String = "^Space"
    ) {
        self.model = model
        self.tools = tools
        self.leaderLabel = leaderLabel
        keys = KeymapEngine(
            style: keyStyle == .tmux ? .tmux : .helix,
            tools: tools.map { ToolKeyInfo(key: $0.key, label: $0.label) },
            paneKeys: paneKeys
        )
        pushToTalk.onClip = { [weak self] clip in self?.deliver(clip) }
        pushToTalk.onFailure = { [weak self] error in self?.voiceFailed(error) }
        pushToTalk.onChange = { [weak self] in self?.updateVoiceStatus() }
    }

    /// Where push-to-talk is.
    var voice: PushToTalk.State { pushToTalk.state }

    var keyStyle: KeyStyle { keys.style() }

    // MARK: - What the window reports

    /// The chrome terminal's grid changed size.
    func resized(to grid: ChromeGrid) {
        self.grid = grid
    }

    /// The time for the status line, `HH:mm`.
    func setClock(_ time: String) {
        if clock != time { clock = time }
    }

    /// The chrome terminal cleared itself; draw everything again.
    func chromeNeedsRedraw() {
        redraws += 1
    }

    /// Where the last frame put the panes and tabs, for focus moves and
    /// clicks.
    func rendered(panes: [ChromePaneRect], tabs: [ChromeTabHit]) {
        lastPanes = panes
        lastTabs = tabs
    }

    /// Whether keys go to the chrome (line editor, screen or picker).
    var isTyping: Bool {
        switch mode {
        case .talk, .command, .newWork, .picker: true
        default: false
        }
    }

    /// Where to land after a talk line, prompt, picker or question: Helix
    /// stays in NOR, tmux goes back to the pane.
    private var restingMode: Mode {
        keyStyle == .tmux ? .pane : .keys
    }

    /// With no work there's nothing else to show: the new-work screen is the
    /// screen. Call after the model changes.
    func syncWithModel() {
        // A tool belongs to its work: switching work hides it.
        if let shownTool, shownTool.workspace != model.selectedWorkspaceID { hideTool() }
        let hasWork = model.selectedWorkspace?.agents.isEmpty == false
        if !hasWork, mode != .newWork, mode != .command, mode != .keys {
            openNewWork()
            screenIsEmptyState = true
        } else if hasWork, screenIsEmptyState, mode == .newWork {
            closeNewWork()
        }
    }

    // MARK: - Keys

    /// Ctrl+Space. Enters Makima's keys; pressed again it leaves them (and
    /// in tmux style, with nothing being typed, goes through to the pane).
    /// Pressed while typing, typing resumes after voice.
    func toggleKeys() {
        switch mode {
        case .keys:
            if keyStyle == .tmux, resumeMode == nil, !keys.isPending() { host?.sendLeaderKeyToFocusedPane() }
            leaveKeys()
        case .confirm:
            break
        case .tool:
            // Out of a tool: hide it (it keeps running) and stay in
            // Makima's keys, ready for another tool or a work.
            hideTool()
            resumeMode = nil
            mode = .keys
            keys.reset()
            notice = nil
            revision += 1
        default:
            resumeMode = isTyping ? mode : nil
            mode = .keys
            keys.reset()
            notice = nil
            revision += 1
        }
    }

    /// One key while in Makima's keys, named like Helix's key map: `h`,
    /// `G`, `space`, `enter`, `esc`, `left`, `C-w`.
    func press(_ token: String) {
        guard mode == .keys else { return }
        let wasPending = keys.isPending()
        let outcome = keys.feed(key: token)
        revision += 1
        switch outcome {
        case .pending:
            return
        case .cancelled:
            // Esc closes a minor mode; with none open it leaves the keys.
            if !wasPending { leaveKeys() }
        case .unbound:
            if keyStyle == .tmux { leaveKeys() }
        case .action(let action):
            let resume = resumeMode
            if keyStyle == .tmux || action == .insert || (action == .voice && resume != nil) {
                leaveKeys()
            } else if resume != nil, !(resume == .tool && Self.keepsTool(action)) {
                // Any other action abandons what was being typed; tool
                // actions (zoom, help) keep the tool to go back to.
                resumeMode = nil
            }
            perform(action)
            syncWithModel()
        }
    }

    /// Back to what was being typed, or to the pane.
    private func leaveKeys() {
        keys.reset()
        mode = resumeMode ?? .pane
        resumeMode = nil
        revision += 1
    }

    func perform(_ action: KeyAction) {
        // Anything about the panes or the work puts the tool away first.
        if shownTool != nil, !Self.keepsTool(action) { hideTool() }
        switch action {
        case .insert: if mode == .keys { leaveKeys() }
        case .focus(let direction):
            if let from = model.selectedAgentID,
                let next = TerminalChrome.neighbor(of: from, direction, in: lastPanes)
            {
                model.select(next)
            }
        case .nextAgent: model.selectAdjacentAgent(offset: 1)
        case .previousAgent: model.selectAdjacentAgent(offset: -1)
        case .nextWork: model.selectAdjacentWorkspace(offset: 1)
        case .previousWork: model.selectAdjacentWorkspace(offset: -1)
        case .work(let number): model.selectWorkspace(number: Int(number))
        case .workPicker: openWorkPicker()
        case .newWork: openNewWork()
        case .newChild: model.addChild()
        case .zoom:
            if shownTool != nil {
                toolZoomed.toggle()
                if mode == .keys { leaveKeys() }
            } else {
                model.toggleZoom()
            }
        case .tool(let index): toggleTool(Int(index))
        case .talkToAll: enterTyping(.talk(.all))
        case .command: enterTyping(.command)
        case .close: askToClose()
        case .help: showHelp.toggle()
        case .voice: toggleVoice()
        }
    }

    // MARK: - Tools

    private static func keepsTool(_ action: KeyAction) -> Bool {
        switch action {
        case .tool, .zoom, .help, .voice, .insert: true
        default: false
        }
    }

    /// Opens tool `index` over the work on screen, in its folder; if it's
    /// already showing, hides it. Hidden tools keep running (`keep`).
    func toggleTool(_ index: Int) {
        guard tools.indices.contains(index) else { return }
        guard let workspace = model.selectedWorkspace else {
            notice = "open a work first; tools run in its folder"
            return
        }
        let ref = ToolRef(workspace: workspace.id, tool: index)
        if shownTool == ref {
            hideTool()
            // Back to typing into the agent, whichever key style.
            keys.reset()
            mode = .pane
            return
        }
        shownTool = ref
        toolZoomed = false
        resumeMode = nil
        keys.reset()
        mode = .tool
        showHelp = false
        notice = nil
    }

    /// Puts the floating window away; keys go back to the pane.
    func hideTool() {
        shownTool = nil
        toolZoomed = false
        if mode == .tool { mode = .pane }
        if resumeMode == .tool { resumeMode = nil }
    }

    /// The tool's program ended: its window closes, and a missing program
    /// (exit 127 from the shell) is reported.
    func toolExited(_ ref: ToolRef, code: Int32?) {
        if shownTool == ref { hideTool() }
        let label = tools.indices.contains(ref.tool) ? tools[ref.tool].label : "the tool"
        switch code {
        case 127: notice = "\(label) isn't installed (command not found)"
        case let code? where code != 0: notice = "\(label) exited with code \(code)"
        default: break
        }
    }

    /// The floating window for the chrome to draw, if a tool is showing.
    var chromeFloat: ChromeFloat? {
        guard let shownTool, tools.indices.contains(shownTool.tool) else { return nil }
        let tool = tools[shownTool.tool]
        let folder = model.workspaces.first { $0.id == shownTool.workspace }?.folder ?? "~"
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let shortFolder = folder.hasPrefix(home) ? "~" + folder.dropFirst(home.count) : folder
        let hint = "\(leaderLabel) hides"
        return ChromeFloat(
            title: tool.label, folder: shortFolder, percent: tool.percent, zoomed: toolZoomed, hint: hint)
    }

    // MARK: - New work

    /// Shows the welcome screen; Esc returns to running work, if any.
    func openNewWork() {
        screenIsEmptyState = false
        let hasWork = model.workspaces.contains { !$0.agents.isEmpty }
        screen = model.newWorkScreen(canClose: hasWork)
        mode = .newWork
        showHelp = false
        notice = nil
        revision += 1
    }

    /// Back to the work, typing into its lead.
    private func closeNewWork() {
        screen = nil
        mode = .pane
        revision += 1
    }

    private func handle(_ event: NewWorkEvent) {
        let started =
            switch event {
            case .start(let request): model.startWork(request)
            case .reopen(let workID): model.reopenWork(id: workID)
            case .close: true
            }
        if started {
            closeNewWork()
        } else {
            screen?.setNotice(model.lastError?.localizedDescription)
        }
    }

    // MARK: - Work picker

    /// A fuzzy list of open work (Helix's buffer picker).
    func openWorkPicker() {
        let items = model.workspaces.map { work in
            let agents = work.agents.count == 1 ? "1 agent" : "\(work.agents.count) agents"
            return PickerItemInfo(
                id: work.id.rawValue,
                label: work.name,
                detail: [agents, work.folder.map(Self.abbreviatingHome)].compactMap(\.self).joined(separator: " · ")
            )
        }
        guard !items.isEmpty else { return }
        resumeMode = nil
        picker = Picker(title: "work", items: items)
        mode = .picker
        showHelp = false
        revision += 1
    }

    private func closePicker() {
        picker = nil
        mode = restingMode
        revision += 1
    }

    private static func abbreviatingHome(_ path: String) -> String {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        return path.hasPrefix(home) ? "~" + path.dropFirst(home.count) : path
    }

    // MARK: - Confirm

    private func askToClose() {
        guard let id = model.selectedAgentID, let agent = model.agent(id: id) else { return }
        mode = .confirm(Confirmation(question: "close \(agent.name)?", agent: id))
    }

    /// The answer to a y/n question.
    func answer(_ yes: Bool) {
        guard case .confirm(let confirmation) = mode else { return }
        mode = restingMode
        if yes { model.removeAgent(id: confirmation.agent) }
        syncWithModel()
    }

    // MARK: - Typing

    func enterTyping(_ newMode: Mode) {
        mode = newMode
        showHelp = false
        notice = nil
        pendingVoice = nil
        editor.setText(text: "")
        mirrorEditor()
    }

    /// Leaves the talk line or prompt.
    func cancelTyping() {
        mode = restingMode
        notice = nil
        pendingVoice = nil
        syncWithModel()
    }

    /// Raw bytes from the chrome terminal while typing.
    func feed(_ input: Data) {
        if mode == .newWork, let screen {
            for event in screen.feed(input) { handle(event) }
            revision += 1
            return
        }
        if mode == .picker, let picker {
            switch picker.feed(input: input) {
            case .chosen(let id):
                closePicker()
                model.selectWorkspace(id: WorkspaceID(rawValue: id))
            case .cancelled: closePicker()
            case nil: revision += 1
            }
            return
        }
        guard isTyping else { return }
        if notice != nil { notice = nil }
        for event in editor.feed(input: input) {
            switch event {
            case .submitted(let line): submit(line)
            case .cancelled: cancelTyping()
            }
        }
        mirrorEditor()
    }

    private func mirrorEditor() {
        lineText = editor.text()
        lineCursor = editor.cursor()
    }

    private func submit(_ line: String) {
        switch mode {
        case .talk(let target):
            if let mistake = say(line, defaultTarget: target) {
                notice = mistake
                editor.setText(text: line)
            } else {
                mode = restingMode
            }
        case .command: run(line)
        default: break
        }
    }

    /// Delivers a talk-line message and saves it with the work. Returns the
    /// mistake, if any.
    @discardableResult
    func say(_ line: String, defaultTarget: TalkTarget = .lead) -> String? {
        guard let workspace = model.selectedWorkspace else { return "there's no work on screen" }
        switch TerminalChrome.route(line, defaultTarget: defaultTarget, in: workspace) {
        case .success(let routed):
            for agent in routed.recipients { host?.send(routed.text, to: agent) }
            model.recordMessage(routed.text, to: routed.recipients, voice: pendingVoice)
            pendingVoice = nil
            return nil
        case .failure(let mistake):
            return mistake.message
        }
    }

    private func run(_ line: String) {
        guard !line.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        switch TerminalChrome.parse(command: line) {
        case .success(.new(let name, let lead, let children, let child)):
            // `lead` is `nil` for the configured default.
            if model.startWork(named: name, lead: lead, children: Int(children), childPreset: child) {
                mode = .pane
            } else {
                notice = model.lastError?.localizedDescription
                editor.setText(text: line)
            }
        case .success(.add(let preset, let count)):
            model.addChildren(Int(count), presetID: preset)
            mode = restingMode
        case .success(.help):
            showHelp = true
        case .failure(let mistake):
            notice = mistake.message
            editor.setText(text: line)
        }
    }

    // MARK: - Voice

    /// Push-to-talk: the first press records, the second stops, transcribes
    /// and puts the text where it belongs: the focused brief question, the
    /// message being typed, or the focused agent's own prompt (unsent, to
    /// read first).
    func toggleVoice() {
        guard let host else { return }
        pushToTalk.toggle(host: host)
    }

    private func deliver(_ clip: VoiceClip) {
        if mode == .newWork, let screen {
            screen.add(clip)
            revision += 1
            return
        }
        if case .talk = mode {
            editor.setText(text: (lineText.isEmpty ? "" : lineText + " ") + clip.transcript)
            pendingVoice = clip
            mirrorEditor()
            return
        }
        guard let agent = model.selectedAgentID else {
            notice = "focus an agent to speak to it"
            return
        }
        host?.type(clip.transcript, into: agent)
        model.recordMessage(clip.transcript, to: [agent], voice: clip)
        if mode == .keys { leaveKeys() }
    }

    private func voiceFailed(_ error: any Error) {
        let message = (error as? any LocalizedError)?.errorDescription ?? String(describing: error)
        if let screen {
            screen.setNotice(message)
            revision += 1
        } else {
            notice = message
        }
    }

    /// What the status line says about voice, if anything.
    var voiceStatus: String? {
        switch voice {
        case .idle: nil
        case .recording: keyStyle == .tmux ? "● recording · ^Space v to stop" : "● recording · ^Space space v to stop"
        case .transcribing: "transcribing …"
        }
    }

    private func updateVoiceStatus() {
        screen?.setVoiceStatus(voiceStatus)
        revision += 1
    }
}
