import Foundation
import MakimaCore
import MakimaCoreBindings
import Testing

@testable import MakimaUI

/// Runs the model against the real Rust core: it is in-memory and fast, so
/// these also cover the FFI boundary.
@Suite struct AppModelTests {
    let events = CoreEventStream()
    let model: AppModel
    let eventTask: Task<Void, Never>

    init() {
        let model = AppModel(core: CoreClient(events: events))
        let stream = events.changes
        eventTask = Task { await model.run(changes: stream) }
        self.model = model
    }

    /// Lets the event task apply what the core has emitted until `condition`
    /// holds. Events are already buffered, so this needs only a few yields.
    func settle(until condition: () -> Bool) async {
        for _ in 0..<1_000 where !condition() {
            await Task.yield()
        }
    }

    /// Starts work with `count` agents and waits until they're all applied.
    func start(_ name: String, preset: String = "shell", count: Int = 1) async throws -> Workspace {
        #expect(model.startWork(named: name, lead: preset, children: count - 1))
        await settle { model.workspaces.last?.name == name && model.workspaces.last?.agents.count == count }
        let workspace = try #require(model.workspaces.last)
        #expect(workspace.name == name)
        return workspace
    }

    @Test func startsOnTheNewWorkScreen() {
        #expect(model.workspaces.isEmpty)
        #expect(model.selectedAgentID == nil)
        #expect(model.presets.map(\.id).contains("claude"))
    }

    @Test func startingWorkLaunchesEveryAgentAndFocusesTheFirst() async throws {
        let workspace = try await start("Fix flaky CI", preset: "claude", count: 3)

        #expect(workspace.name == "Fix flaky CI")
        #expect(workspace.agents.map(\.name) == ["claude", "claude 2", "claude 3"])
        #expect(workspace.agents.allSatisfy { $0.command?.hasPrefix("claude --session-id ") == true })
        #expect(Set(workspace.agents.compactMap(\.sessionID)).count == 3, "each agent gets its own session")
        #expect(model.selectedAgentID == workspace.agents.first?.id)
    }

    @Test func blankWorkIsRejected() {
        #expect(!model.startWork(named: "   "))
        #expect(model.lastError == AppError(operation: .startWork, cause: .emptyName))
        #expect(model.lastError?.localizedDescription == "Give it a name first.")
        #expect(model.selectedAgentID == nil)
    }

    @Test func newAgentsJoinTheWorkOnScreen() async throws {
        _ = try await start("Auth")
        let ci = try await start("CI")

        model.addAgent(presetID: "codex")
        model.addCustomAgent(command: "htop -d 5")
        await settle { model.workspaces.last?.agents.count == 3 }

        #expect(model.workspaces.first?.agents.count == 1)
        #expect(model.selectedWorkspace?.id == ci.id)
        #expect(model.workspaces.last?.agents.map(\.name) == ["shell", "codex", "htop"])
    }

    /// Intents issued back to back, before any core event is applied, must
    /// still land in the work that was just started.
    @Test func chainedIntentsDoNotWaitForEvents() async throws {
        model.startWork(named: "Burst")
        model.addAgent(presetID: "claude")
        model.addCustomAgent(command: "make test")
        await settle { model.workspaces.first?.agents.count == 3 }

        // No lead named: the configured one (`[work] lead`, claude by default).
        #expect(model.workspaces.first?.agents.map(\.name) == ["claude", "claude 2", "make"])
        #expect(model.selectedAgentID == model.workspaces.first?.agents.last?.id)
    }

    @Test func navigationWrapsAndZoomFollows() async throws {
        let workspace = try await start("Agents", count: 3)
        let ids = workspace.agents.map(\.id)

        model.selectAdjacentAgent(offset: -1)
        #expect(model.selectedAgentID == ids[2])

        model.toggleZoom()
        model.selectAdjacentAgent(offset: 1)
        #expect(model.selectedAgentID == ids[0])
        #expect(model.zoomedAgentID == ids[0])

        model.toggleZoom()
        #expect(model.zoomedAgentID == nil)
    }

    @Test func closingTheFocusedAgentStaysInItsWork() async throws {
        let workspace = try await start("Auth", count: 2)
        model.select(workspace.agents[1].id)

        model.removeAgent(id: workspace.agents[1].id)
        await settle { model.workspaces.first?.agents.count == 1 }

        #expect(model.selectedAgentID == workspace.agents[0].id)
    }

    @Test func nonZeroExitsFail() async throws {
        let id = try #require(try await start("Build").agents.first?.id)

        model.agentStarted(id: id)
        await settle { model.agent(id: id)?.status == .working }
        model.agentExited(id: id, code: 2)
        await settle { model.agent(id: id)?.status != .working }
        #expect(model.agent(id: id)?.status == .failed(reason: "exited with code 2"))
    }

    @Test func backgroundAgentsAskingForInputAreFlaggedUntilFocused() async throws {
        let workspace = try await start("Agents", count: 2)
        let background = workspace.agents[1].id
        model.agentStarted(id: background)
        await settle { model.agent(id: background)?.status == .working }

        model.agentNeedsAttention(id: background, message: "Claude needs your permission")
        await settle { model.agent(id: background)?.status != .working }
        #expect(model.agent(id: background)?.status == .inputRequired(prompt: "Claude needs your permission"))

        model.select(background)
        await settle { model.agent(id: background)?.status == .working }
        #expect(model.agent(id: background)?.status == .working)
    }

    @Test func reopeningWithoutStorageSaysSo() {
        let id = WorkspaceID(rawValue: UUID().uuidString.lowercased())
        #expect(!model.reopenWork(id: id))
        #expect(model.lastError == AppError(operation: .reopenWork, cause: .storageOff))
    }

    @Test func workNavigationWraps() async throws {
        let auth = try await start("auth")
        let ci = try await start("ci")
        let docs = try await start("docs")
        #expect(model.selectedWorkspaceID == docs.id)

        model.selectAdjacentWorkspace(offset: 1)
        #expect(model.selectedWorkspaceID == auth.id, "wraps to the first")
        model.selectAdjacentWorkspace(offset: -1)
        #expect(model.selectedWorkspaceID == docs.id, "and back to the last")
        model.selectAdjacentWorkspace(offset: -1)
        #expect(model.selectedWorkspaceID == ci.id)
    }
}
