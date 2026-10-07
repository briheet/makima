import Foundation
import MakimaCore
import MakimaVoice
import Testing

@testable import MakimaUI

/// Stands in for on-device speech recognition.
private struct FakeTranscriber: Transcriber {
    let provider = "fake"
    let text: String

    func transcribe(_ recording: URL, language: String?) async throws -> Transcript {
        guard !text.isEmpty else { throw VoiceError.nothingHeard }
        return Transcript(text: text, language: language ?? "en-US")
    }
}

@Suite struct VoiceInputTests {
    let recording = Recording(url: URL(fileURLWithPath: "/tmp/note.m4a"), durationMilliseconds: 1500)

    @Test func clipsCarryTheTranscriptAndWhoMadeIt() async throws {
        let input = VoiceInput(transcriber: FakeTranscriber(text: "fix the login bug"))

        let clip = try await input.clip(for: recording, language: "en-GB")

        #expect(
            clip
                == VoiceClip(
                    recording: "/tmp/note.m4a",
                    durationMilliseconds: 1500,
                    transcript: "fix the login bug",
                    provider: "fake",
                    language: "en-GB"
                )
        )
    }

    @Test func transcriberErrorsReachTheCaller() async {
        let input = VoiceInput(transcriber: FakeTranscriber(text: ""))
        await #expect(throws: VoiceError.nothingHeard) {
            try await input.clip(for: recording, language: nil)
        }
    }
}
