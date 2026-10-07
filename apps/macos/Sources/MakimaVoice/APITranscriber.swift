import Foundation
import Security

/// Transcription through an OpenAI-compatible `/audio/transcriptions`
/// endpoint: OpenAI, Groq, or a local Whisper server.
public struct APITranscriber: Transcriber {
    public let provider: String
    private let endpoint: URL
    private let model: String
    private let apiKey: String
    private let session: URLSession

    /// - Parameters:
    ///   - baseURL: e.g. `https://api.openai.com/v1`.
    ///   - provider: Stored with each transcript, e.g. `openai`.
    public init(baseURL: URL, model: String, apiKey: String, provider: String = "openai", session: URLSession = .shared)
    {
        endpoint = baseURL.appending(path: "audio/transcriptions")
        self.model = model
        self.apiKey = apiKey
        self.provider = provider
        self.session = session
    }

    /// Reads, uploads and decodes off the main thread.
    @concurrent
    public func transcribe(_ recording: URL, language: String?) async throws -> Transcript {
        let audio: Data
        do {
            audio = try Data(contentsOf: recording)
        } catch {
            throw VoiceError.unreadableRecording(recording)
        }
        // The API takes ISO 639-1 ("en"), not BCP 47 ("en-US").
        let languageCode = language.flatMap { $0.split(separator: "-").first.map(String.init) }
        var form = MultipartForm()
        form.add("model", model)
        form.add("response_format", "json")
        if let languageCode { form.add("language", languageCode) }
        form.finish(file: "file", name: recording.lastPathComponent, type: "audio/mp4", data: audio)

        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.setValue("Bearer \(apiKey)", forHTTPHeaderField: "Authorization")
        request.setValue(form.contentType, forHTTPHeaderField: "Content-Type")

        let (body, response): (Data, URLResponse)
        do {
            (body, response) = try await session.upload(for: request, from: form.body)
        } catch let error as URLError {
            throw VoiceError.network(error.code)
        } catch {
            throw VoiceError.network(.unknown)
        }
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(status) else {
            let message = (try? JSONDecoder().decode(APIError.self, from: body))?.error.message
            throw VoiceError.rejected(status: status, message: message)
        }
        let text: String
        do {
            text = try JSONDecoder().decode(Reply.self, from: body).text
                .trimmingCharacters(in: .whitespacesAndNewlines)
        } catch {
            throw VoiceError.unreadableReply
        }
        guard !text.isEmpty else { throw VoiceError.nothingHeard }
        return Transcript(text: text, language: language)
    }

    private struct Reply: Decodable {
        let text: String
    }

    private struct APIError: Decodable {
        struct Detail: Decodable { let message: String }
        let error: Detail
    }
}

/// Where API keys come from: an environment variable, else the macOS
/// Keychain (apps opened from the Dock don't see the shell's variables).
public enum APIKey {
    /// The key in `environmentVariable`, else the Keychain's generic password
    /// for service `makima` and account `account`.
    public static func find(
        environmentVariable: String,
        account: String,
        environment: [String: String] = ProcessInfo.processInfo.environment
    ) -> String? {
        if let key = environment[environmentVariable], !key.isEmpty { return key }
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "makima",
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess, let data = item as? Data else {
            return nil
        }
        return String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

/// A `multipart/form-data` body.
private struct MultipartForm {
    private let boundary = "makima-\(UUID().uuidString)"
    private(set) var body = Data()

    var contentType: String { "multipart/form-data; boundary=\(boundary)" }

    mutating func add(_ name: String, _ value: String) {
        append("--\(boundary)\r\nContent-Disposition: form-data; name=\"\(name)\"\r\n\r\n\(value)\r\n")
    }

    /// Adds the file and closes the form; nothing can be added after it.
    mutating func finish(file name: String, name fileName: String, type: String, data: Data) {
        append(
            "--\(boundary)\r\nContent-Disposition: form-data; name=\"\(name)\"; filename=\"\(fileName)\"\r\n"
                + "Content-Type: \(type)\r\n\r\n")
        body.append(data)
        append("\r\n--\(boundary)--\r\n")
    }

    private mutating func append(_ text: String) {
        body.append(Data(text.utf8))
    }
}
