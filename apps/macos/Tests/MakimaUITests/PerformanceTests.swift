import Foundation
import MakimaCore
import MakimaCoreBindings
import Testing

@testable import MakimaUI

/// Timings for the app's hot paths, through the real Rust core and FFI.
/// Opt-in (`MAKIMA_BENCH=1`, see `just bench`), and meant for release
/// builds: debug timings mislead.
@Suite(.enabled(if: ProcessInfo.processInfo.environment["MAKIMA_BENCH"] == "1"), .serialized)
struct PerformanceTests {
    let events = CoreEventStream()
    let model: AppModel
    let tui: TUIController
    let eventTask: Task<Void, Never>

    init() async {
        let model = AppModel(core: CoreClient(events: events))
        let stream = events.changes
        eventTask = Task { await model.run(changes: stream) }
        self.model = model
        tui = TUIController(model: model)
        model.startWork(named: "bench", lead: "shell", children: 8)
        for _ in 0..<1_000 where model.selectedWorkspace?.agents.count != 9 {
            await Task.yield()
        }
    }

    /// Runs `body` `runs` times after a warm-up; prints the median and p95.
    func measure(_ name: String, runs: Int = 500, _ body: () -> Void) {
        for _ in 0..<20 { body() }
        let clock = ContinuousClock()
        var samples: [Duration] = []
        samples.reserveCapacity(runs)
        for _ in 0..<runs {
            samples.append(clock.measure(body))
        }
        samples.sort()
        let median = samples[runs / 2]
        let p95 = samples[runs * 95 / 100]
        let micros = { (d: Duration) in Double(d.components.attoseconds) / 1e12 + Double(d.components.seconds) * 1e6 }
        print(
            String(format: "BENCH %-52@ median %8.1f µs   p95 %8.1f µs", name as NSString, micros(median), micros(p95)))
    }

    @Test func hotPaths() throws {
        #expect(model.selectedWorkspace?.agents.count == 9)
        let displays: [AgentID: PaneDisplay] = [:]

        measure("chromeState (Swift, 9 agents)") { _ = tui.chromeState(panes: displays) }

        let state = tui.chromeState(panes: displays)
        measure("render via FFI 146×48 (9 agents)") {
            _ = TerminalChrome.render(state, keys: tui.keys, screen: nil, picker: nil, columns: 146, rows: 48)
        }
        measure("render via FFI 240×70 (9 agents)") {
            _ = TerminalChrome.render(state, keys: tui.keys, screen: nil, picker: nil, columns: 240, rows: 70)
        }
        measure("a full repaint: chromeState + render 146×48") {
            let state = tui.chromeState(panes: displays)
            _ = TerminalChrome.render(state, keys: tui.keys, screen: nil, picker: nil, columns: 146, rows: 48)
        }

        let agent = try #require(model.selectedWorkspace?.agents[3])
        measure("AppModel.apply(agentUpdated)", runs: 5_000) { model.apply(.agentUpdated(agent)) }

        measure("NOR key (space, esc) through the keymap", runs: 5_000) {
            _ = tui.keys.feed(key: "space")
            _ = tui.keys.feed(key: "esc")
        }
    }
}
