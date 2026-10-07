import Foundation
import MakimaCore
import MakimaCoreBindings
import Testing

@testable import MakimaUI

/// Records what the TUI asks the window to do; "transcribes" every
/// recording as `spokenText`.
private final class RecordingHost: TUIHost {
    var sent: [(text: String, agent: AgentID)] = []
    var typed: [(text: String, agent: AgentID)] = []
    var leaderKeys = 0
    var recordings = 0
    var spokenText = "run the tests"

    func send(_ text: String, to agent: AgentID) { sent.append((text, agent)) }
    func type(_ text: String, into agent: AgentID) { typed.append((text, agent)) }
    func sendLeaderKeyToFocusedPane() { leaderKeys += 1 }
    func startRecording() async throws { recordings += 1 }
    func finishRecording() async throws -> VoiceClip {
        VoiceClip(
            recording: "/tmp/clip.m4a",
            durationMilliseconds: 1200,
            transcript: spokenText,
            provider: "test",
            language: "en-US"
        )
    }
}

/// The TUI's modes, key maps, talk line and command prompt, against the
/// real Rust core and chrome logic. Tmux style unless a test says Helix.
@Suite struct TUIControllerTests {
    let events = CoreEventStream()
    let model: AppModel
    let tui: TUIController
    private let host = RecordingHost()
    let eventTask: Task<Void, Never>
    let storageRoot: URL

    init() {
        // Real storage in a temporary folder, so briefs and messages are saved.
        storageRoot = FileManager.default.temporaryDirectory
            .appendingPathComponent("makima-tests-\(UUID().uuidString)")
        let model = AppModel(core: CoreClient(events: events, storage: .at(storageRoot.path)))
        let stream = events.changes
        eventTask = Task { await model.run(changes: stream) }
        self.model = model
        tui = TUIController(model: model, keyStyle: .tmux)
        tui.host = host
    }

    /// A controller over the same model with Helix keys.
    func helix() -> TUIController {
        let helix = TUIController(model: model, keyStyle: .helix)
        helix.host = host
        return helix
    }

    /// Ctrl+Space, then `keys` one by one.
    func press(_ keys: String..., on controller: TUIController? = nil) {
        let tui = controller ?? self.tui
        tui.toggleKeys()
        for key in keys { tui.press(key) }
    }

    func settle(until condition: () -> Bool) async {
        for _ in 0..<1_000 where !condition() {
            await Task.yield()
        }
    }

    /// Types `text` into the chrome terminal, then Enter.
    func type(_ text: String) {
        tui.feed(Data((text + "\r").utf8))
    }

    func startTeam(children: Int = 2) async {
        #expect(model.startWork(named: "auth", lead: "shell", children: children))
        await settle { model.selectedWorkspace?.agents.count == children + 1 }
        tui.syncWithModel()
    }

    @Test func withNoWorkTheWelcomeScreenShows() async {
        tui.syncWithModel()
        #expect(tui.mode == .newWork)
        #expect(tui.screen != nil)
        #expect(tui.chromeState(panes: [:]).mode == .newWork)

        // Enter twice: a quick start with the default agent.
        type("")
        tui.feed(Data("\r".utf8))
        await settle { model.selectedWorkspace != nil }
        #expect(tui.mode == .pane)
        #expect(tui.screen == nil)
        #expect(model.selectedWorkspace?.lead?.command?.hasPrefix("claude") == true)
    }

    @Test func templatesStartATeamWithABrief() async throws {
        tui.syncWithModel()
        // Templates → Fix (2nd): its first question has focus.
        // Helix forms open at the top: j down to the first question (past
        // folder, name, lead, children, they run, permits), i to type, Esc,
        // G to start.
        for keys in ["t", "2", "j", "j", "j", "j", "j", "j", "i", "login loops", "\u{1b}", "G", "\r"] {
            tui.feed(Data(keys.utf8))
        }
        await settle { model.selectedWorkspace?.agents.count == 2 }
        let workspace = try #require(model.selectedWorkspace)
        #expect(workspace.children.first?.role == .tester)
        #expect(workspace.lead?.command?.contains("Read the brief") == true)
        let briefs = try FileManager.default.subpathsOfDirectory(atPath: storageRoot.path).filter {
            $0.hasSuffix("brief.md")
        }
        let brief = try String(
            contentsOfFile: storageRoot.appendingPathComponent(try #require(briefs.first)).path, encoding: .utf8)
        #expect(brief.contains("login loops"))
    }

    @Test func newCommandStartsALeadAndChildren() async throws {
        await startTeam(children: 2)
        let workspace = try #require(model.selectedWorkspace)

        #expect(workspace.name == "auth")
        #expect(workspace.lead?.name == "shell")
        #expect(workspace.children.map(\.name) == ["shell 2", "shell 3"])
        #expect(tui.mode == .pane)
        #expect(model.selectedAgentID == workspace.lead?.id)
    }

    @Test func badCommandsShowANotice() {
        tui.perform(.command)
        type("launch rockets")
        #expect(tui.mode == .command)
        #expect(tui.notice?.contains("unknown command") == true)
        #expect(tui.lineText == "launch rockets", "the line is kept for fixing")
    }

    @Test func messagesGoToEveryoneUnlessRetargeted() async throws {
        await startTeam(children: 2)
        let workspace = try #require(model.selectedWorkspace)

        tui.perform(.talkToAll)
        #expect(tui.mode == .talk(.all))
        type("summarise")
        #expect(host.sent.map(\.agent) == workspace.agents.map(\.id))
        #expect(host.sent.allSatisfy { $0.text == "summarise" })
        #expect(tui.mode == .pane)

        host.sent.removeAll()
        tui.perform(.talkToAll)
        type("@lead plan the refactor")
        #expect(host.sent.map(\.agent) == [workspace.lead?.id])

        host.sent.removeAll()
        tui.perform(.talkToAll)
        type("@2 just you")
        #expect(host.sent.map(\.agent) == [workspace.agents[2].id])
    }

    @Test func unknownTargetsKeepTheLineOpen() async {
        await startTeam(children: 1)
        tui.perform(.talkToAll)
        type("@nobody hi")
        #expect(host.sent.isEmpty)
        #expect(tui.mode == .talk(.all))
        #expect(tui.notice == "no agent called @nobody")
    }

    @Test func leaderTwiceReachesThePane() async {
        await startTeam(children: 0)
        tui.toggleKeys()
        #expect(tui.mode == .keys)
        tui.toggleKeys()
        #expect(host.leaderKeys == 1)
        #expect(tui.mode == .pane)
    }

    @Test func closingAsksFirst() async throws {
        await startTeam(children: 1)
        let child = try #require(model.selectedWorkspace?.children.first)
        model.select(child.id)

        press("x")
        #expect(tui.mode == .confirm(.init(question: "close shell 2?", agent: child.id)))
        tui.answer(false)
        #expect(model.agent(id: child.id) != nil)

        tui.perform(.close)
        tui.answer(true)
        await settle { model.agent(id: child.id) == nil }
        #expect(model.selectedWorkspace?.agents.count == 1)
    }

    @Test func helpToggles() async {
        await startTeam(children: 0)
        tui.perform(.help)
        #expect(tui.showHelp)
        #expect(tui.chromeState(panes: [:]).showHelp)
    }

    @Test func chromeStateDescribesTheTeam() async throws {
        await startTeam(children: 2)
        let workspace = try #require(model.selectedWorkspace)
        let lead = try #require(workspace.lead)
        model.agentStarted(id: lead.id)
        await settle { model.agent(id: lead.id)?.status == .working }

        let state = tui.chromeState(panes: [lead.id: PaneDisplay(title: "vim", isClosed: false)])

        #expect(state.tabs.map(\.label) == ["auth"])
        #expect(state.panes.map(\.role) == [.lead, .child(number: 1), .child(number: 2)])
        #expect(state.panes.first?.detail == "vim")
        #expect(state.panes.first?.focused == true)
        #expect(state.counts.working == 1)
        #expect(state.mode == .pane)
    }

    @Test func pushToTalkTypesIntoTheFocusedAgent() async throws {
        await startTeam(children: 1)
        let child = try #require(model.selectedWorkspace?.children.first)
        model.select(child.id)

        press("v")
        await settle { host.recordings == 1 }
        #expect(tui.voice == .recording)
        tui.toggleVoice()
        await settle { tui.voice == .idle }

        #expect(tui.mode == .pane, "no talk line")
        #expect(host.typed.map(\.agent) == [child.id])
        #expect(host.typed.first?.text == "run the tests")
        #expect(host.sent.isEmpty, "typed, not sent: Enter is the user's")
    }

    @Test func voiceWhileTypingResumesTyping() async {
        await startTeam(children: 0)
        tui.perform(.talkToAll)
        tui.feed(Data("please".utf8))
        press("v")
        #expect(tui.mode == .talk(.all), "voice returns to the line being typed")
        await settle { host.recordings == 1 }
        tui.toggleVoice()
        await settle { tui.voice == .idle }
        #expect(tui.lineText == "please run the tests")
    }

    @Test func workNumbersJumpBetweenTabs() async throws {
        await startTeam(children: 0)
        model.startWork(named: "ci")
        await settle { model.workspaces.count == 2 && model.workspaces.last?.agents.isEmpty == false }

        press("1")
        #expect(model.selectedWorkspace?.name == "auth")
        tui.perform(.work(number: 2))
        #expect(model.selectedWorkspace?.name == "ci")
    }

    // MARK: - Helix keys

    @Test func norLeavesAgentsAloneAndStaysUntilInsert() async throws {
        await startTeam(children: 2)
        let workspace = try #require(model.selectedWorkspace)
        let tui = helix()
        tui.syncWithModel()

        press("j", on: tui)
        tui.press("l")
        #expect(model.selectedAgentID == workspace.lead?.id, "agents are Ctrl+h/j/k/l's, not NOR's")
        #expect(tui.mode == .keys, "NOR stays")
        tui.press("i")
        #expect(tui.mode == .pane)
    }

    @Test func paneKeysReturnWhenCtrlHjklIsOff() async throws {
        await startTeam(children: 1)
        let workspace = try #require(model.selectedWorkspace)
        let tui = TUIController(model: model, keyStyle: .helix, paneKeys: true)
        tui.host = host
        tui.rendered(panes: [], tabs: [])
        press("j", on: tui)
        #expect(tui.mode == .keys)
        tui.press("i")
        #expect(model.selectedWorkspace?.id == workspace.id)
    }

    @Test func helixSwitchesWorkWithBracketsAndThePicker() async throws {
        await startTeam(children: 0)
        model.startWork(named: "ci")
        await settle { model.workspaces.count == 2 && model.workspaces.last?.agents.isEmpty == false }
        #expect(model.selectedWorkspace?.name == "ci")
        let tui = helix()

        press("[", on: tui)
        #expect(model.selectedWorkspace?.name == "auth")
        tui.press("]")
        #expect(model.selectedWorkspace?.name == "ci")

        tui.press("space")
        tui.press("b")
        #expect(tui.mode == .picker)
        #expect(tui.picker != nil)
        tui.feed(Data("auth\r".utf8))
        #expect(model.selectedWorkspace?.name == "auth")
        #expect(tui.mode == .keys, "Helix lands back in NOR")
        #expect(tui.picker == nil)
    }

    @Test func helixEscapeClosesMenusThenNormal() async {
        await startTeam(children: 0)
        let tui = helix()
        press("space", on: tui)
        tui.press("esc")
        #expect(tui.mode == .keys, "Esc closes the space menu")
        tui.press("q")
        #expect(tui.mode == .keys, "unbound keys are ignored in NOR")
        tui.press("esc")
        #expect(tui.mode == .pane)
    }

    @Test func helixTalkReturnsToNormal() async throws {
        await startTeam(children: 1)
        let workspace = try #require(model.selectedWorkspace)
        let tui = helix()
        press("space", "a", on: tui)
        #expect(tui.mode == .talk(.all))
        tui.feed(Data("status?\r".utf8))
        #expect(host.sent.map(\.agent) == workspace.agents.map(\.id))
        #expect(tui.mode == .keys)
    }

    @Test func theAddCommandGrowsTheTeamWithoutALimit() async throws {
        await startTeam(children: 1)
        tui.perform(.command)
        type("add 20")
        await settle { model.selectedWorkspace?.agents.count == 22 }
        #expect(model.selectedWorkspace?.agents.count == 22)
        #expect(tui.mode == .pane)

        tui.perform(.command)
        type("add nope 2")
        #expect(model.lastError == AppError(operation: .addAgent, cause: .unknownPreset("nope")))
    }

    // MARK: - Tools

    static let tools = [
        AppConfig.Tool(id: "git", label: "lazygit", command: "lazygit", key: "g", percent: 90, keeps: true),
        AppConfig.Tool(id: "shell", label: "shell", command: nil, key: "s", percent: 80, keeps: true),
    ]

    func withTools(_ style: AppConfig.Keys.Style) -> TUIController {
        let tui = TUIController(model: model, keyStyle: style, tools: Self.tools)
        tui.host = host
        return tui
    }

    @Test func spaceGOpensATheWorksToolAndHidesIt() async throws {
        await startTeam(children: 1)
        let workspace = try #require(model.selectedWorkspace)
        let tui = withTools(.helix)

        press("space", "g", on: tui)
        #expect(tui.mode == .tool)
        #expect(tui.shownTool == .init(workspace: workspace.id, tool: 0))
        let state = tui.chromeState(panes: [:])
        #expect(state.mode == .tool)
        #expect(state.float?.title == "lazygit")
        #expect(state.float?.percent == 90)
        #expect(state.float?.hint == "^Space hides")
        #expect(state.panes.allSatisfy { !$0.focused }, "the panes behind are quiet")

        tui.perform(.zoom)
        #expect(tui.toolZoomed)
        #expect(tui.mode == .tool, "zooming (from the menu) keeps the tool")

        // Ctrl+Space in a tool: hide it, stay in Makima's keys…
        tui.toggleKeys()
        #expect(tui.shownTool == nil)
        #expect(tui.mode == .keys, "not back in the agent")
        // …so another tool is one key away.
        tui.press("space")
        tui.press("s")
        #expect(tui.shownTool == .init(workspace: workspace.id, tool: 1))
        #expect(tui.mode == .tool)
    }

    @Test func toolsBelongToTheirWork() async throws {
        await startTeam(children: 0)
        model.startWork(named: "ci")
        await settle { model.workspaces.count == 2 && model.workspaces.last?.agents.isEmpty == false }
        let tui = withTools(.helix)
        press("space", "s", on: tui)
        #expect(tui.shownTool?.workspace == model.selectedWorkspaceID)

        press("[", on: tui)
        tui.syncWithModel()
        #expect(tui.shownTool == nil, "switching work puts the tool away")
    }

    @Test func tmuxPrefixOpensToolsAndMissingProgramsAreReported() async throws {
        await startTeam(children: 0)
        let tui = withTools(.tmux)
        press("g", on: tui)
        let shown = try #require(tui.shownTool)
        #expect(tui.mode == .tool)

        tui.toolExited(shown, code: 127)
        #expect(tui.shownTool == nil)
        #expect(tui.mode == .pane)
        #expect(tui.notice == "lazygit isn't installed (command not found)")
    }

    @Test func toolsNeedAWork() {
        let tui = withTools(.helix)
        tui.toggleTool(0)
        #expect(tui.shownTool == nil)
        #expect(tui.notice?.contains("open a work first") == true)
    }
}
