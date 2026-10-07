public import MakimaCoreBindings

/// What the app asks of the Rust core. ``CoreClient`` is the real one; tests
/// can stand in a fake to drive failures the core rarely produces.
public protocol CoreService: Sendable {
    var presets: [AgentPreset] { get }
    var workspaces: [Workspace] { get }
    /// The preset new work's lead runs when none is named (`[work] lead`).
    var defaultLead: String { get }
    var recordingsDirectory: String? { get }
    func newWorkScreen(canClose: Bool) -> NewWorkScreen
    func startWork(_ request: NewWorkRequest) throws(CoreFailure) -> Workspace
    func reopenWork(id: WorkspaceID) throws(CoreFailure) -> Workspace
    func recordMessage(_ text: String, to recipients: [AgentID], in workspace: WorkspaceID, voice: VoiceClip?)
        throws(CoreFailure)
    func addAgent(preset presetID: String, to workspace: WorkspaceID) throws(CoreFailure) -> Agent
    func addAgent(named name: String, command: String, to workspace: WorkspaceID) throws(CoreFailure) -> Agent
    func setStatus(_ status: Agent.Status, of agent: AgentID) throws(CoreFailure)
    func recordExit(of agent: AgentID, code: Int32?) throws(CoreFailure)
    func remove(_ agent: AgentID) throws(CoreFailure)
}

/// The app's handle on the Rust core: the generated `Core` object behind an
/// API of Swift domain types and ``CoreFailure`` errors.
///
/// Calls are synchronous and cheap (in-memory); changes also arrive as
/// ``CoreChange`` values through the ``CoreEventStream`` it was made with.
public final class CoreClient: CoreService {
    /// Where work is saved.
    public enum Storage: Sendable {
        /// Nothing is saved (tests, previews).
        case off
        /// `~/.local/share/makima`, or the config's `[storage] root`.
        case standard
        /// Everything under a folder.
        case at(String)
    }

    private let core: Core

    /// Reads the config and opens the store, so it can take a moment; make
    /// it off the main thread at launch.
    /// - Parameters:
    ///   - configPath: The config file to read; `nil` uses built-in defaults.
    ///   - storage: Where work is saved; `.off` saves nothing (tests).
    public init(events: CoreEventStream, configPath: String? = nil, storage: Storage = .off) {
        let mode: StorageMode =
            switch storage {
            case .off: .off
            case .standard: .default
            case .at(let root): .at(root: root)
            }
        core = Core(sink: events, configPath: configPath, storage: mode)
    }

    public var defaultLead: String {
        core.defaultLead()
    }

    /// The welcome screen, template picker and new-work form, offering this
    /// core's presets, templates, recent work and folders.
    public func newWorkScreen(canClose: Bool) -> NewWorkScreen {
        NewWorkScreen(core.newWorkFlow(canClose: canClose))
    }

    /// Starts and saves work.
    /// - Throws: ``CoreFailure``.
    public func startWork(_ request: NewWorkRequest) throws(CoreFailure) -> Workspace {
        try translatingErrors { Workspace(try core.startWork(request: request.info)) }
    }

    /// Reopens saved work, resuming agents' sessions where possible.
    /// - Throws: ``CoreFailure``.
    public func reopenWork(id: WorkspaceID) throws(CoreFailure) -> Workspace {
        try translatingErrors { Workspace(try core.reopenWork(workId: id)) }
    }

    /// Records what the user sent, with its recording if spoken.
    /// - Throws: ``CoreFailure``.
    public func recordMessage(
        _ text: String,
        to recipients: [AgentID],
        in workspace: WorkspaceID,
        voice: VoiceClip? = nil
    ) throws(CoreFailure) {
        try translatingErrors {
            try core.recordMessage(
                workspaceId: workspace,
                text: text,
                recipients: recipients,
                voice: voice.map { VoiceClipInfo($0, question: nil) }
            )
        }
    }

    /// Where to record voice before it belongs to a work; `nil` when
    /// storage is off.
    public var recordingsDirectory: String? {
        core.recordingsDir()
    }

    public var presets: [AgentPreset] {
        core.presets().map(AgentPreset.init)
    }

    public var workspaces: [Workspace] {
        core.workspaces().map(Workspace.init)
    }

    /// - Throws: ``CoreFailure``.
    public func addAgent(preset presetID: String, to workspace: WorkspaceID) throws(CoreFailure) -> Agent {
        try translatingErrors { Agent(try core.addPresetAgent(workspaceId: workspace, presetId: presetID)) }
    }

    /// Adds an agent running `command`, named `name`.
    /// - Throws: ``CoreFailure``.
    public func addAgent(named name: String, command: String, to workspace: WorkspaceID) throws(CoreFailure)
        -> Agent
    {
        try translatingErrors {
            Agent(
                try core.addAgent(
                    workspaceId: workspace,
                    name: name,
                    role: .custom(name: name),
                    command: command
                )
            )
        }
    }

    /// - Throws: ``CoreFailure``.
    public func setStatus(_ status: Agent.Status, of agent: AgentID) throws(CoreFailure) {
        try translatingErrors { _ = try core.setAgentStatus(agentId: agent, status: AgentStatus(status)) }
    }

    /// Records an exit; `code` is `nil` when unknown.
    /// - Throws: ``CoreFailure``.
    public func recordExit(of agent: AgentID, code: Int32?) throws(CoreFailure) {
        try translatingErrors { _ = try core.recordAgentExit(agentId: agent, exitCode: code) }
    }

    /// - Throws: ``CoreFailure``.
    public func remove(_ agent: AgentID) throws(CoreFailure) {
        try translatingErrors { try core.removeAgent(agentId: agent) }
    }

    private func translatingErrors<T>(_ body: () throws -> T) throws(CoreFailure) -> T {
        do {
            return try body()
        } catch let error as CoreError {
            throw CoreFailure(error)
        } catch {
            throw .other(String(describing: error))
        }
    }
}

// MARK: - Conversions from the generated FFI types

extension Agent {
    init(_ info: AgentInfo) {
        self.init(
            id: info.id,
            name: info.name,
            role: Role(info.role),
            command: info.command,
            status: Status(info.status),
            sessionID: info.sessionId,
            permission: info.permission.map(Permission.init)
        )
    }
}

extension Agent.Role {
    init(_ role: AgentRole) {
        switch role {
        case .planner: self = .planner
        case .implementer: self = .implementer
        case .reviewer: self = .reviewer
        case .tester: self = .tester
        case .researcher: self = .researcher
        case .shell: self = .shell
        case .custom(let name): self = .custom(name)
        }
    }
}

extension Agent.Permission {
    init(_ mode: PermissionMode) {
        switch mode {
        case .manual: self = .manual
        case .acceptEdits: self = .acceptEdits
        case .plan: self = .plan
        case .auto: self = .auto
        case .bypass: self = .bypass
        }
    }
}

extension Agent.Status {
    init(_ status: AgentStatus) {
        switch status {
        case .starting: self = .starting
        case .idle: self = .idle
        case .working: self = .working
        case .inputRequired(let prompt): self = .inputRequired(prompt: prompt)
        case .completed: self = .completed
        case .failed(let reason): self = .failed(reason: reason)
        case .interrupted: self = .interrupted
        }
    }
}

extension AgentStatus {
    init(_ status: Agent.Status) {
        switch status {
        case .starting: self = .starting
        case .idle: self = .idle
        case .working: self = .working
        case .inputRequired(let prompt): self = .inputRequired(prompt: prompt)
        case .completed: self = .completed
        case .failed(let reason): self = .failed(reason: reason)
        case .interrupted: self = .interrupted
        }
    }
}

extension Workspace {
    init(_ info: WorkspaceInfo) {
        self.init(id: info.id, name: info.name, folder: info.folder, agents: info.agents.map(Agent.init))
    }
}

extension AgentPreset {
    init(_ info: AgentPresetInfo) {
        self.init(id: info.id, label: info.label, command: info.command)
    }
}

extension CoreChange {
    init(_ event: CoreEvent) {
        switch event {
        case .workspaceCreated(let workspace): self = .workspaceCreated(Workspace(workspace))
        case .agentAdded(let workspaceID, let agent): self = .agentAdded(Agent(agent), to: workspaceID)
        case .agentUpdated(let agent): self = .agentUpdated(Agent(agent))
        case .agentRemoved(let agentID): self = .agentRemoved(agentID)
        }
    }
}

extension CoreFailure {
    init(_ error: CoreError) {
        switch error {
        case .EmptyName: self = .emptyName
        case .WorkspaceNotFound(let id): self = .workspaceNotFound(id)
        case .AgentNotFound(let id): self = .agentNotFound(id)
        case .AgentFinished(let id): self = .agentFinished(id)
        case .UnknownPreset(let id): self = .unknownPreset(id)
        case .InvalidColor(let value): self = .invalidColor(value)
        case .UnknownWork(let id): self = .unknownWork(id)
        case .StorageOff: self = .storageOff
        case .Storage(let message): self = .storage(message)
        case .Other(let message): self = .other(message)
        }
    }
}
