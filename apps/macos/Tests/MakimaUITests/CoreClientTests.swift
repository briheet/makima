import MakimaCore
import Testing

@testable import MakimaUI

/// The Swift edge of the FFI: typed IDs, domain types and typed failures.
@Suite struct CoreClientTests {
    let client = CoreClient(events: CoreEventStream())

    @Test func failuresArriveTypedWithTheirIDs() throws {
        let workspace = try client.startWork(.quick(named: "CI", lead: "shell", folder: "/tmp"))
        let agent = try client.addAgent(preset: "shell", to: workspace.id)
        try client.remove(agent.id)

        #expect(throws: CoreFailure.agentNotFound(agent.id)) { try client.remove(agent.id) }
        #expect(throws: CoreFailure.unknownPreset("nope")) { try client.addAgent(preset: "nope", to: workspace.id) }
        #expect(throws: CoreFailure.emptyName) {
            try client.startWork(.quick(named: " ", lead: "shell", folder: "/tmp"))
        }
    }

    @Test func idsRoundTripThroughRust() throws {
        let workspace = try client.startWork(.quick(named: "Auth", lead: "claude", folder: "/tmp"))
        let agent = try #require(workspace.lead)

        let stored = try #require(client.workspaces.first)
        #expect(stored.id == workspace.id)
        #expect(stored.agents == [agent])
        #expect(agent.command == "claude --session-id \(agent.sessionID ?? "")")
        #expect(agent.permission == nil, "no [permissions] configured")
    }
}
