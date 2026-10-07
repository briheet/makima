import Foundation
import MakimaVoice
import Testing

/// Answers transcription requests without a network. The host picks the
/// answer: `echo.test` describes the request it got, `denied.test` refuses.
///
/// `@unchecked Sendable`: `URLProtocol` isn't `Sendable`, but URL loading
/// drives each instance from one thread and this subclass adds no state.
private nonisolated final class StubAPI: URLProtocol, @unchecked Sendable {
    override class func canInit(with _: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        guard let url = request.url else {
            client?.urlProtocol(self, didFailWithError: URLError(.badURL))
            return
        }
        let (status, body): (Int, String)
        switch url.host() {
        case "denied.test":
            (status, body) = (401, #"{"error":{"message":"Incorrect API key"}}"#)
        default:
            let form = Self.body(of: request)
            let summary = [
                request.value(forHTTPHeaderField: "Authorization") ?? "-",
                url.path(),
                form.contains("whisper-1") ? "model" : "-",
                form.contains("name=\"language\"\r\n\r\nen\r\n") ? "en" : "-",
                form.contains("fake audio") ? "audio" : "-",
            ]
            (status, body) = (200, #"{"text":" \#(summary.joined(separator: " ")) "}"#)
        }
        guard let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: nil, headerFields: nil) else {
            client?.urlProtocol(self, didFailWithError: URLError(.badServerResponse))
            return
        }
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: Data(body.utf8))
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}

    private static func body(of request: URLRequest) -> String {
        if let body = request.httpBody { return String(decoding: body, as: UTF8.self) }
        guard let stream = request.httpBodyStream else { return "" }
        stream.open()
        defer { stream.close() }
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)
        while stream.hasBytesAvailable {
            let count = stream.read(&buffer, maxLength: buffer.count)
            guard count > 0 else { break }
            data.append(buffer, count: count)
        }
        return String(decoding: data, as: UTF8.self)
    }
}

@Suite struct APITranscriberTests {
    let session: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [StubAPI.self]
        return URLSession(configuration: configuration)
    }()

    func recording() throws -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("makima-\(UUID().uuidString).m4a")
        try Data("fake audio".utf8).write(to: url)
        return url
    }

    @Test func sendsTheModelLanguageAndKey() async throws {
        let transcriber = APITranscriber(
            baseURL: try #require(URL(string: "https://echo.test/v1")), model: "whisper-1", apiKey: "sk-test",
            session: session)

        let transcript = try await transcriber.transcribe(try recording(), language: "en-US")

        #expect(transcript.text == "Bearer sk-test /v1/audio/transcriptions model en audio")
        #expect(transcript.language == "en-US")
        #expect(transcriber.provider == "openai")
    }

    @Test func apiErrorsCarryTheirMessage() async throws {
        let transcriber = APITranscriber(
            baseURL: try #require(URL(string: "https://denied.test/v1")), model: "whisper-1", apiKey: "bad",
            session: session)
        await #expect(throws: VoiceError.rejected(status: 401, message: "Incorrect API key")) {
            try await transcriber.transcribe(try recording(), language: nil)
        }
    }

    @Test func keysComeFromTheEnvironmentFirst() {
        let key = APIKey.find(
            environmentVariable: "MAKIMA_TEST_KEY", account: "makima-tests-none",
            environment: ["MAKIMA_TEST_KEY": "sk-env"])
        #expect(key == "sk-env")
        #expect(
            APIKey.find(environmentVariable: "MAKIMA_TEST_KEY", account: "makima-tests-none", environment: [:]) == nil)
    }
}
