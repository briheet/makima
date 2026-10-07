import Foundation
import MakimaCore
import Observation
import os

/// A failed user intent: which operation, and why. Shown to the user via
/// `errorDescription`; the operation is the context, like `anyhow` in Rust.
struct AppError: LocalizedError, Equatable {
    enum Operation: String, Sendable {
        case startWork = "start work"
        case addAgent = "add agent"
        case reopenWork = "reopen work"
        case recordMessage = "save the message"
        case closeAgent = "close agent"
        case recordExit = "record exit"
        case updateStatus = "update agent status"
    }

    let operation: Operation
    let cause: CoreFailure

    var errorDescription: String? {
        switch cause {
        case .emptyName: "Give it a name first."
        case .unknownPreset(let id): "There's no agent preset called \(id)."
        case .invalidColor(let value): "\(value) isn't a colour (#rrggbb)."
        case .storage(let message): "Couldn't save the work: \(message)"
        case .storageOff: "Couldn't \(operation.rawValue): saving is off."
        case .unknownWork: "That work isn't saved any more."
        case .agentFinished: "That agent has already finished."
        case .workspaceNotFound, .agentNotFound: "Couldn't \(operation.rawValue): it no longer exists."
        case .other(let message): "Couldn't \(operation.rawValue): \(message)"
        }
    }
}

/// The app's view of the Rust core.
///
/// State flows one way: the core emits changes, `apply(_:)` folds them into
/// `workspaces`, and views read from here. User intents go back to the core
/// as method calls; nothing in the UI mutates `workspaces` directly.
@Observable
public final class AppModel {
    private(set) var workspaces: [Workspace] = []
    /// The work on screen; `nil` shows the new-work screen. Stored rather
    /// than derived from the agent, so intents issued before the core's
    /// changes arrive still target the right work.
    private(set) var selectedWorkspaceID: WorkspaceID?
    /// The focused agent, within the selected work.
    private(set) var selectedAgentID: AgentID?
    /// An agent filling the whole main area, if any.
    private(set) var zoomedAgentID: AgentID?
    /// The most recent failure, for display. Cleared by the next success.
    private(set) var lastError: AppError?
    /// Kinds of agent the user can start (shell, Claude Code, …).
    public let presets: [AgentPreset]

    @ObservationIgnored private let core: any CoreService
    @ObservationIgnored private let logger = Logger(subsystem: "io.makima.app", category: "model")

    public init(core: any CoreService) {
        self.core = core
        presets = core.presets
        workspaces = core.workspaces
    }

    /// Applies changes until `changes` finishes. Run it for the app's lifetime.
    public func run(changes: AsyncStream<CoreChange>) async {
        for await change in changes {
            apply(change)
        }
    }

    /// Folds one core change into `workspaces`.
    func apply(_ change: CoreChange) {
        switch change {
        case .workspaceCreated(let workspace):
            if !workspaces.contains(where: { $0.id == workspace.id }) { workspaces.append(workspace) }
        case .agentAdded:
            for index in workspaces.indices { workspaces[index].apply(change) }
        case .agentUpdated(let agent):
            logger.info("\(agent.name, privacy: .public) is \(agent.status.label, privacy: .public)")
            for index in workspaces.indices { workspaces[index].apply(change) }
        case .agentRemoved(let agentID):
            let workspace = workspace(containing: agentID)
            for index in workspaces.indices { workspaces[index].apply(change) }
            if zoomedAgentID == agentID { zoomedAgentID = nil }
            if selectedAgentID == agentID {
                // Stay in the same work, on whatever is left in it.
                let remaining = workspace.flatMap { ws in workspaces.first { $0.id == ws.id } }
                selectedAgentID = remaining?.agents.first?.id
            }
        }
    }

    // MARK: - Queries

    func agent(id: AgentID) -> Agent? {
        workspaces.lazy.flatMap(\.agents).first { $0.id == id }
    }

    func workspace(containing agentID: AgentID) -> Workspace? {
        workspaces.first { $0.agents.contains { $0.id == agentID } }
    }

    /// The workspace on screen.
    var selectedWorkspace: Workspace? {
        selectedWorkspaceID.flatMap { id in workspaces.first { $0.id == id } }
    }

    // MARK: - Navigation

    /// Focuses an agent and shows its work; an agent that was asking for
    /// input is back to work.
    func select(_ agentID: AgentID) {
        if let workspace = workspace(containing: agentID), selectedWorkspaceID != workspace.id {
            selectedWorkspaceID = workspace.id
        }
        if selectedAgentID != agentID { selectedAgentID = agentID }
        if case .inputRequired = agent(id: agentID)?.status {
            setStatus(.working, of: agentID)
        }
    }

    /// Shows a workspace, focusing its first agent.
    func selectWorkspace(id: WorkspaceID) {
        guard selectedWorkspaceID != id, let workspace = workspaces.first(where: { $0.id == id }) else { return }
        selectedWorkspaceID = id
        zoomedAgentID = nil
        selectedAgentID = workspace.agents.first?.id
    }

    /// Shows the work in tab `number` (1-based), if there is one.
    func selectWorkspace(number: Int) {
        guard workspaces.indices.contains(number - 1) else { return }
        selectWorkspace(id: workspaces[number - 1].id)
    }

    /// Shows the work `offset` tabs along from the current one, wrapping.
    func selectAdjacentWorkspace(offset: Int) {
        guard !workspaces.isEmpty else { return }
        let index = workspaces.firstIndex { $0.id == selectedWorkspaceID } ?? 0
        let count = workspaces.count
        selectWorkspace(id: workspaces[(index + offset % count + count) % count].id)
    }

    /// Moves focus `offset` agents along the selected workspace, wrapping.
    func selectAdjacentAgent(offset: Int) {
        guard let agents = selectedWorkspace?.agents, !agents.isEmpty,
            let index = agents.firstIndex(where: { $0.id == selectedAgentID })
        else { return }
        let next = agents[(index + offset % agents.count + agents.count) % agents.count]
        select(next.id)
        if zoomedAgentID != nil { zoomedAgentID = next.id }
    }

    /// Toggles the selected agent filling the main area.
    func toggleZoom() {
        zoomedAgentID = zoomedAgentID == nil ? selectedAgentID : nil
    }

    // MARK: - Intents

    /// Starts and saves work, focusing its lead. Returns whether it
    /// succeeded; `lastError` says why not.
    @discardableResult
    func startWork(_ request: NewWorkRequest) -> Bool {
        perform(.startWork) {
            showStarted(try core.startWork(request))
        }
    }

    /// Starts work without the new-work screen (the `makima>` prompt, launch
    /// flags): a lead running `lead` (`[work] lead` when `nil`) plus
    /// `children` from `childPreset` (the lead's when `nil`), in `folder`
    /// (home when `nil`).
    @discardableResult
    public func startWork(
        named name: String,
        lead: String? = nil,
        children: Int = 0,
        childPreset: String? = nil,
        folder: String? = nil
    ) -> Bool {
        startWork(
            .quick(
                named: name,
                lead: lead ?? core.defaultLead,
                children: children,
                childPreset: childPreset,
                folder: folder ?? FileManager.default.homeDirectoryForCurrentUser.path
            )
        )
    }

    /// Reopens saved work, resuming its agents' sessions where possible.
    @discardableResult
    func reopenWork(id: WorkspaceID) -> Bool {
        perform(.reopenWork) {
            showStarted(try core.reopenWork(id: id))
        }
    }

    /// The new-work screen; `canClose` when there's running work to return to.
    func newWorkScreen(canClose: Bool) -> NewWorkScreen {
        core.newWorkScreen(canClose: canClose)
    }

    /// Saves what the user sent to agents, with the recording if spoken.
    func recordMessage(_ text: String, to recipients: [AgentID], voice: VoiceClip? = nil) {
        guard let workspaceID = selectedWorkspaceID else { return }
        perform(.recordMessage) {
            try core.recordMessage(text, to: recipients, in: workspaceID, voice: voice)
        }
    }

    /// Where voice is recorded before it's saved with a work.
    var recordingsDirectory: String? {
        core.recordingsDirectory
    }

    private func showStarted(_ workspace: Workspace) {
        selectedWorkspaceID = workspace.id
        selectedAgentID = workspace.lead?.id
        zoomedAgentID = nil
    }

    /// Adds a child to the work on screen, running the same thing as its lead.
    func addChild() {
        guard let workspace = selectedWorkspace else { return }
        guard let lead = workspace.lead else {
            addAgent(presetID: "shell", to: workspace.id)
            return
        }
        if let preset = presets.first(where: { $0.command == lead.command }) {
            addAgent(presetID: preset.id, to: workspace.id)
        } else if let command = lead.command {
            addCustomAgent(command: command, to: workspace.id)
        }
    }

    /// Adds `count` children to the work on screen, running `presetID` (what
    /// the lead runs when `nil`). There's no limit: teams grow while the work
    /// runs.
    func addChildren(_ count: Int, presetID: String? = nil) {
        if let presetID, !presets.contains(where: { $0.id == presetID }) {
            lastError = AppError(operation: .addAgent, cause: .unknownPreset(presetID))
            return
        }
        for _ in 0..<max(0, count) {
            if let presetID { addAgent(presetID: presetID) } else { addChild() }
        }
    }

    /// Adds an agent from a preset to `workspaceID`, or to the workspace on screen.
    func addAgent(presetID: String, to workspaceID: WorkspaceID? = nil) {
        guard let workspaceID = workspaceID ?? selectedWorkspaceID else { return }
        perform(.addAgent) {
            focusNew(try core.addAgent(preset: presetID, to: workspaceID), in: workspaceID)
        }
    }

    /// Adds an agent running an arbitrary command, named after the program.
    public func addCustomAgent(command: String, to workspaceID: WorkspaceID? = nil) {
        let command = command.trimmingCharacters(in: .whitespaces)
        guard !command.isEmpty, let workspaceID = workspaceID ?? selectedWorkspaceID else { return }
        let name = command.split(separator: " ").first.map(String.init) ?? command
        perform(.addAgent) {
            focusNew(try core.addAgent(named: name, command: command, to: workspaceID), in: workspaceID)
        }
    }

    func removeAgent(id: AgentID) {
        perform(.closeAgent) { try core.remove(id) }
    }

    // MARK: - Process lifecycle (reported by terminals)

    /// The agent's terminal process is running.
    func agentStarted(id: AgentID) {
        guard agent(id: id)?.status == .starting else { return }
        setStatus(.working, of: id)
    }

    /// The agent's process exited; `code` is `nil` when unknown.
    func agentExited(id: AgentID, code: Int32?) {
        guard let agent = agent(id: id), !agent.status.isFinished else { return }
        perform(.recordExit) { try core.recordExit(of: id, code: code) }
    }

    /// The agent's program asked for attention (a notification or the bell).
    /// Only agents the user isn't looking at are flagged.
    func agentNeedsAttention(id: AgentID, message: String) {
        guard id != selectedAgentID, agent(id: id)?.status == .working else { return }
        setStatus(.inputRequired(prompt: message), of: id)
    }

    // MARK: - Helpers

    /// Focuses an agent that may not have reached `workspaces` yet.
    private func focusNew(_ agent: Agent, in workspaceID: WorkspaceID) {
        selectedWorkspaceID = workspaceID
        selectedAgentID = agent.id
        zoomedAgentID = nil
    }

    private func setStatus(_ status: Agent.Status, of id: AgentID) {
        perform(.updateStatus) { try core.setStatus(status, of: id) }
    }

    @discardableResult
    private func perform(_ operation: AppError.Operation, _ body: () throws -> Void) -> Bool {
        do {
            try body()
            lastError = nil
            return true
        } catch {
            // Closures don't infer typed throws yet; every core call throws
            // `CoreFailure`.
            let failure = error as? CoreFailure ?? .other(String(describing: error))
            logger.error(
                "\(operation.rawValue, privacy: .public) failed: \(String(describing: failure), privacy: .public)"
            )
            lastError = AppError(operation: operation, cause: failure)
            return false
        }
    }
}
