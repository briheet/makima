public import MakimaCoreBindings

/// Bridges the Rust core's event callbacks into an `AsyncStream` of
/// ``CoreChange``.
///
/// Rust calls `onEvent` on whichever thread made the change; consumers
/// iterate `changes` on their own actor. `Sendable` without `@unchecked`:
/// its only stored properties are the stream and its continuation.
public final class CoreEventStream: CoreEventSink {
    public let changes: AsyncStream<CoreChange>
    private let continuation: AsyncStream<CoreChange>.Continuation

    public init() {
        // Unbounded: dropping a change would leave the UI out of sync with
        // the core, and changes are small and infrequent.
        (changes, continuation) = AsyncStream.makeStream(of: CoreChange.self)
    }

    public func onEvent(event: CoreEvent) {
        continuation.yield(CoreChange(event))
    }

    /// Ends the stream; later changes are dropped.
    public func finish() {
        continuation.finish()
    }
}
