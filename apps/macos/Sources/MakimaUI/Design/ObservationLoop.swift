import Observation

/// Runs `apply` now, and again whenever observable state it read changes —
/// the AppKit counterpart of a SwiftUI view re-rendering.
///
/// The loop owns one task, which it cancels when cancelled or deallocated,
/// so the owner keeps it as a property for as long as it should run.
/// `apply` should capture its owner weakly.
public final class ObservationLoop {
    private let task: Task<Void, Never>

    init(_ apply: @escaping @MainActor () -> Void) {
        // Change notifications arrive synchronously, before the new value is
        // stored; they're queued here and re-applied on the next turn.
        let (changes, continuation) = AsyncStream.makeStream(of: Void.self, bufferingPolicy: .bufferingNewest(1))
        let track = { withObservationTracking(apply) { continuation.yield() } }
        track()
        task = Task {
            for await _ in changes {
                track()
            }
        }
    }

    deinit {
        task.cancel()
    }
}
