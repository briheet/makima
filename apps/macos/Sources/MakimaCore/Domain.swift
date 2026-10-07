public import MakimaCoreBindings

/// The Swift domain model. The app works with these types only; the
/// UniFFI-generated records (`AgentInfo`, `WorkspaceInfo`, …) stay inside
/// this module and are converted at the edge in ``CoreClient``.

/// A long-running worker in a workspace, backed by a terminal process.
public struct Agent: Identifiable, Hashable, Sendable {
    public enum Role: Hashable, Sendable {
        case planner, implementer, reviewer, tester, researcher, shell
        case custom(String)
    }

    /// How much the agent may do without asking.
    public enum Permission: Hashable, Sendable {
        case manual, acceptEdits, plan, auto
        /// Permission checks off (Claude's `--dangerously-skip-permissions`).
        case bypass
    }

    public enum Status: Hashable, Sendable {
        case starting, idle, working
        case inputRequired(prompt: String)
        case completed
        case failed(reason: String)
        case interrupted

        /// Matches the core's rule: finished agents can't change status.
        public var isFinished: Bool {
            switch self {
            case .completed, .failed: true
            default: false
            }
        }
    }

    public let id: AgentID
    public let name: String
    public let role: Role
    /// `nil` runs the user's login shell.
    public let command: String?
    public let status: Status
    /// The program's own session id (Claude Code's), when pinned.
    public let sessionID: String?
    /// The mode it was started in, when its program has one.
    public let permission: Permission?
}

/// A piece of work and the agents organised around it. The first agent is
/// the lead; the rest are its children (the same rule as the Rust core).
public struct Workspace: Identifiable, Hashable, Sendable {
    public let id: WorkspaceID
    public let name: String
    /// Where the agents work.
    public let folder: String?
    public internal(set) var agents: [Agent]

    public var lead: Agent? { agents.first }
    public var children: ArraySlice<Agent> { agents.dropFirst() }
}

/// A kind of agent the user can start: a shell, Claude Code, …
public struct AgentPreset: Identifiable, Hashable, Sendable {
    public let id: String
    public let label: String
    /// `nil` runs the user's login shell.
    public let command: String?

    public init(id: String, label: String, command: String?) {
        self.id = id
        self.label = label
        self.command = command
    }
}

/// A change in the core, delivered by ``CoreEventStream``.
public enum CoreChange: Sendable, Equatable {
    case workspaceCreated(Workspace)
    case agentAdded(Agent, to: WorkspaceID)
    case agentUpdated(Agent)
    case agentRemoved(AgentID)
}

/// Why a core operation failed.
public enum CoreFailure: Error, Hashable, Sendable {
    case emptyName
    case workspaceNotFound(WorkspaceID)
    case agentNotFound(AgentID)
    case agentFinished(AgentID)
    case unknownPreset(String)
    /// A colour that isn't `#rrggbb` or `#rgb`.
    case invalidColor(String)
    /// No saved work has this id.
    case unknownWork(WorkspaceID)
    /// Storage is off, so nothing was saved.
    case storageOff
    /// Saving or loading work failed.
    case storage(String)
    /// Anything the Swift side has no specific case for.
    case other(String)
}

extension Workspace {
    /// Applies `change` if it concerns this workspace.
    public mutating func apply(_ change: CoreChange) {
        switch change {
        case .agentAdded(let agent, let workspaceID) where workspaceID == id:
            agents.append(agent)
        case .agentUpdated(let agent):
            if let index = agents.firstIndex(where: { $0.id == agent.id }) { agents[index] = agent }
        case .agentRemoved(let agentID):
            agents.removeAll { $0.id == agentID }
        default:
            break
        }
    }
}
