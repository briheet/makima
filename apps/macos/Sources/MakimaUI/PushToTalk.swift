import MakimaCore
import Observation

/// Push-to-talk: the first press records, the second stops and
/// transcribes. Owns the task doing it, which ends with it.
@Observable
final class PushToTalk {
    enum State: Equatable {
        case idle, recording, transcribing
    }

    private(set) var state: State = .idle
    /// Gets each transcribed clip.
    @ObservationIgnored var onClip: ((VoiceClip) -> Void)?
    /// Gets why recording or transcribing failed.
    @ObservationIgnored var onFailure: ((any Error) -> Void)?
    /// Runs after every step, for status lines.
    @ObservationIgnored var onChange: (() -> Void)?
    @ObservationIgnored private var task: Task<Void, Never>?

    isolated deinit {
        task?.cancel()
    }

    /// Starts recording through `host`, or stops and transcribes; does
    /// nothing while transcribing.
    func toggle(host: any TUIHost) {
        switch state {
        case .idle:
            state = .recording
            run { [weak host] in
                try await host?.startRecording()
                return nil
            }
        case .recording:
            state = .transcribing
            run { [weak host] in try await host?.finishRecording() }
        case .transcribing:
            return
        }
        onChange?()
    }

    /// Runs `step` in place of any earlier one, reporting its clip or
    /// failure unless it was cancelled.
    private func run(_ step: @escaping () async throws -> VoiceClip?) {
        task?.cancel()
        task = Task { [weak self] in
            do {
                let clip = try await step()
                guard let self, !Task.isCancelled else { return }
                if state == .transcribing { state = .idle }
                if let clip { onClip?(clip) }
            } catch {
                guard let self, !Task.isCancelled else { return }
                state = .idle
                onFailure?(error)
            }
            self?.onChange?()
        }
    }
}
