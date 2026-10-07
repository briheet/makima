import Foundation
import MakimaVoice
import Testing

/// Real on-device transcription of speech generated with macOS's `say`
/// (a named voice: the default system voice can write empty files).
/// Opt-in (`MAKIMA_VOICE_TESTS=1`): it may download the speech model.
@Suite(.enabled(if: ProcessInfo.processInfo.environment["MAKIMA_VOICE_TESTS"] == "1"))
struct VoiceTests {
    @Test func appleTranscribesSpokenAudio() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
            "makima-voice-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let speech = directory.appendingPathComponent("speech.aiff")
        let say = Process()
        say.executableURL = URL(fileURLWithPath: "/usr/bin/say")
        say.arguments = ["-v", "Samantha", "-o", speech.path, "Please run the tests and fix the login bug."]
        try say.run()
        say.waitUntilExit()

        // The same format VoiceRecorder writes: AAC in .m4a, 16 kHz mono.
        let recording = directory.appendingPathComponent("speech.m4a")
        let convert = Process()
        convert.executableURL = URL(fileURLWithPath: "/usr/bin/afconvert")
        convert.arguments = ["-f", "m4af", "-d", "aac@16000", "-c", "1", speech.path, recording.path]
        try convert.run()
        convert.waitUntilExit()

        let transcript = try await AppleTranscriber().transcribe(recording, language: "en-US")

        let words = transcript.text.lowercased()
        #expect(words.contains("test"), "\(transcript.text)")
        #expect(words.contains("login"), "\(transcript.text)")
        #expect(transcript.language?.hasPrefix("en") == true)
    }
}
