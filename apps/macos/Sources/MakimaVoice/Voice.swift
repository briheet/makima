import AVFoundation
import Foundation
import Speech

/// Why voice input didn't work.
public enum VoiceError: LocalizedError, Equatable, Sendable {
    case microphoneDenied
    /// Voice can't be used as configured; says why.
    case unavailable(String)
    case nothingHeard
    /// Stopped with no recording running.
    case notRecording
    /// The recording couldn't be started or saved; the system's reason.
    case recording(String)
    /// The finished recording couldn't be read back.
    case unreadableRecording(URL)
    /// The transcription API couldn't be reached.
    case network(URLError.Code)
    /// The transcription API refused, with its HTTP status and message.
    case rejected(status: Int, message: String?)
    /// The transcription API's answer wasn't a transcript.
    case unreadableReply
    /// On-device transcription failed; the system's reason.
    case transcription(String)

    public var errorDescription: String? {
        switch self {
        case .microphoneDenied: "Makima can't use the microphone; allow it in System Settings › Privacy & Security."
        case .unavailable(let why): why
        case .nothingHeard: "Didn't catch anything; try again."
        case .notRecording: "Nothing was recording."
        case .recording(let why): "Couldn't record: \(why)"
        case .unreadableRecording(let url): "Couldn't read the recording \(url.lastPathComponent)."
        case .network(let code): "Couldn't reach the transcription API: \(URLError(code).localizedDescription)"
        case .rejected(let status, let message):
            "The transcription API answered \(status)\(message.map { ": \($0)" } ?? "")."
        case .unreadableReply: "The transcription API's reply wasn't understood."
        case .transcription(let why): "Transcription failed: \(why)"
        }
    }
}

/// A finished recording.
public struct Recording: Sendable, Equatable {
    public let url: URL
    public let durationMilliseconds: UInt64

    public init(url: URL, durationMilliseconds: UInt64) {
        self.url = url
        self.durationMilliseconds = durationMilliseconds
    }
}

/// Text from a recording.
public struct Transcript: Sendable, Equatable {
    public let text: String
    /// The language actually used, e.g. `en-US`.
    public let language: String?

    public init(text: String, language: String?) {
        self.text = text
        self.language = language
    }
}

/// Speech to text. A protocol so tests (and later, cloud providers) can
/// stand in for the on-device one.
public protocol Transcriber: Sendable {
    /// Stored with each transcript, e.g. `apple`.
    var provider: String { get }
    func transcribe(_ recording: URL, language: String?) async throws -> Transcript
}

/// Records the microphone to an AAC `.m4a` file (16 kHz mono: plenty for
/// speech, small on disk).
@MainActor
public final class VoiceRecorder {
    private var recorder: AVAudioRecorder?

    public init() {}

    /// Starts recording into a new file in `directory`, asking for the
    /// microphone the first time.
    public func start(in directory: URL) async throws {
        guard await Self.microphoneAllowed() else { throw VoiceError.microphoneDenied }
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        } catch {
            throw VoiceError.recording(error.localizedDescription)
        }
        let url = directory.appendingPathComponent("\(UUID().uuidString).m4a")
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatMPEG4AAC,
            AVSampleRateKey: 16_000,
            AVNumberOfChannelsKey: 1,
            AVEncoderAudioQualityKey: AVAudioQuality.high.rawValue,
        ]
        let recorder: AVAudioRecorder
        do {
            recorder = try AVAudioRecorder(url: url, settings: settings)
        } catch {
            throw VoiceError.recording(error.localizedDescription)
        }
        guard recorder.record() else { throw VoiceError.recording("the recorder didn't start") }
        self.recorder = recorder
    }

    /// Stops and returns the recording, if one was running.
    public func stop() -> Recording? {
        guard let recorder else { return nil }
        let seconds = recorder.currentTime
        recorder.stop()
        self.recorder = nil
        return Recording(url: recorder.url, durationMilliseconds: UInt64(max(0, seconds) * 1000))
    }

    private static func microphoneAllowed() async -> Bool {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: true
        case .notDetermined: await AVCaptureDevice.requestAccess(for: .audio)
        default: false
        }
    }
}

/// On-device transcription with macOS 26's `SpeechAnalyzer`: private, free,
/// and good with long recordings. Downloads the language model on first use.
public struct AppleTranscriber: Transcriber {
    public let provider = "apple"

    public init() {}

    @concurrent
    public func transcribe(_ recording: URL, language: String?) async throws -> Transcript {
        guard #available(macOS 26, *) else {
            throw VoiceError.unavailable("On-device transcription needs macOS 26.")
        }
        let requested = language.map(Locale.init(identifier:)) ?? Locale.current
        guard let locale = await SpeechTranscriber.supportedLocale(equivalentTo: requested) else {
            throw VoiceError.unavailable("On-device transcription doesn't support \(requested.identifier) yet.")
        }
        let transcriber = SpeechTranscriber(locale: locale, preset: .transcription)
        do {
            if let download = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
                try await download.downloadAndInstall()
            }
            let file = try AVAudioFile(forReading: recording)
            let analyzer = SpeechAnalyzer(modules: [transcriber])
            // Collect results while the analyzer works through the file,
            // then finalize so the last words are reported too.
            async let text = Self.collect(transcriber)
            if let end = try await analyzer.analyzeSequence(from: file) {
                try await analyzer.finalizeAndFinish(through: end)
            } else {
                await analyzer.cancelAndFinishNow()
            }
            let transcript = try await text
            guard !transcript.isEmpty else { throw VoiceError.nothingHeard }
            return Transcript(text: transcript, language: locale.identifier(.bcp47))
        } catch let error as VoiceError {
            throw error
        } catch {
            throw VoiceError.transcription(error.localizedDescription)
        }
    }

    @available(macOS 26, *)
    private static func collect(_ transcriber: SpeechTranscriber) async throws -> String {
        var text = ""
        for try await result in transcriber.results {
            text += String(result.text.characters)
        }
        return text.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
