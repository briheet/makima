import Observation
import Testing

@testable import MakimaUI

@Observable private final class Counter {
    var value = 0
}

@Suite struct ObservationLoopTests {
    @Test func reappliesWhenObservedStateChanges() async {
        let counter = Counter()
        var seen: [Int] = []
        var loop: ObservationLoop? = ObservationLoop { seen.append(counter.value) }
        #expect(seen == [0])

        counter.value = 1
        for _ in 0..<100 where seen.count < 2 { await Task.yield() }
        #expect(seen == [0, 1])

        // Releasing the loop stops it, as the app does.
        _ = loop
        loop = nil
        counter.value = 2
        for _ in 0..<20 { await Task.yield() }
        #expect(seen == [0, 1])
    }
}
