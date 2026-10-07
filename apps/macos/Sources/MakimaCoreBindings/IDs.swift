/// Identifies an agent. Crosses the FFI as a UUID string (a UniFFI custom
/// type, see `crates/makima-ffi/uniffi.toml`), but is never confused with a
/// workspace ID or any other string in Swift.
public struct AgentID: Hashable, Sendable, CustomStringConvertible {
    public let rawValue: String

    public init(rawValue: String) {
        self.rawValue = rawValue
    }

    public var description: String { rawValue }
}

/// Identifies a workspace. See ``AgentID``.
public struct WorkspaceID: Hashable, Sendable, CustomStringConvertible {
    public let rawValue: String

    public init(rawValue: String) {
        self.rawValue = rawValue
    }

    public var description: String { rawValue }
}
