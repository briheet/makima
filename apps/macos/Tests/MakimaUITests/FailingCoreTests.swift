import MakimaCore
import Testing

@testable import MakimaUI

/// A core whose every change fails with `failure`: failures the real core
/// rarely produces (a full disk, a corrupt database).
private struct FailingCore: CoreService {
    let failure: CoreFailure

    var presets: [AgentPreset] { [AgentPreset(id: "shell", label: "Shell", command: nil)] }
    var workspaces: [Workspace] { [] }
    var defaultLead: String { "shell" }
    var recordingsDirectory: String? { nil }

    func newWorkScreen(canClose: Bool) -> NewWorkScreen {
        CoreClient(events: CoreEventStream()).newWorkScreen(canClose: canClose)
    }

    func startWork(_: NewWorkRequest) throws(CoreFailure) -> Workspace { throw failure }
    func reopenWork(id _: WorkspaceID) throws(CoreFailure) -> Workspace { throw failure }
    func recordMessage(_: String, to _: [AgentID], in _: WorkspaceID, voice _: VoiceClip?) throws(CoreFailure) {
        throw failure
    }
    func addAgent(preset _: String, to _: WorkspaceID) throws(CoreFailure) -> Agent { throw failure }
    func addAgent(named _: String, command _: String, to _: WorkspaceID) throws(CoreFailure) -> Agent {
        throw failure
    }
    func setStatus(_: Agent.Status, of _: AgentID) throws(CoreFailure) { throw failure }
    func recordExit(of _: AgentID, code _: Int32?) throws(CoreFailure) { throw failure }
    func remove(_: AgentID) throws(CoreFailure) { throw failure }
}

@Suite struct FailingCoreTests {
    @Test func storageFailuresAreShownWithTheirCause() {
        let model = AppModel(core: FailingCore(failure: .storage("database: disk I/O error")))

        #expect(!model.startWork(named: "fix login"))
        #expect(model.lastError == AppError(operation: .startWork, cause: .storage("database: disk I/O error")))
        #expect(model.lastError?.errorDescription == "Couldn't save the work: database: disk I/O error")
        #expect(model.selectedWorkspaceID == nil, "nothing half-started is shown")
    }
}
