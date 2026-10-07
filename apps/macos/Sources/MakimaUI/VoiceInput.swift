import Foundation
import MakimaCore
import MakimaVoice

/// Push-to-talk input: records the microphone, then turns the recording
/// into a ``VoiceClip`` with whichever ``Transcriber`` it was given (on-device
/// in the app, a stand-in in tests).
final class VoiceInput {
    private let recorder = VoiceRecorder()
    /// Set from the config before each recording.
    var transcriber: any Transcriber

    init(transcriber: any Transcriber) {
        self.transcriber = transcriber
    }

    /// Starts recording into a new file in `directory`.
    func start(in directory: URL) async throws {
        try await recorder.start(in: directory)
    }

    /// Stops recording and transcribes it.
    func finish(language: String?) async throws -> VoiceClip {
        guard let recording = recorder.stop() else { throw VoiceError.notRecording }
        return try await clip(for: recording, language: language)
    }

    /// Transcribes `recording` into a clip that remembers who transcribed
    /// it and in which language.
    func clip(for recording: Recording, language: String?) async throws -> VoiceClip {
        let transcript = try await transcriber.transcribe(recording.url, language: language)
        return VoiceClip(
            recording: recording.url.path,
            durationMilliseconds: recording.durationMilliseconds,
            transcript: transcript.text,
            provider: transcriber.provider,
            language: transcript.language
        )
    }
}
