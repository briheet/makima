import Foundation
public import MakimaCoreBindings

/// A transcribed voice recording, saved with the work or message it
/// belongs to.
public struct VoiceClip: Hashable, Sendable {
    /// The recording file; moved into the work when saved.
    public let recording: String
    public let durationMilliseconds: UInt64?
    public let transcript: String
    /// Who transcribed it, e.g. `apple`.
    public let provider: String
    /// The language actually used, e.g. `en-US`.
    public let language: String?

    public init(
        recording: String,
        durationMilliseconds: UInt64?,
        transcript: String,
        provider: String,
        language: String?
    ) {
        self.recording = recording
        self.durationMilliseconds = durationMilliseconds
        self.transcript = transcript
        self.provider = provider
        self.language = language
    }
}

/// Everything needed to start a work: from the new-work screen, or
/// ``quick(named:lead:children:childPreset:folder:)`` for scripts.
public struct NewWorkRequest: Equatable, Sendable {
    let info: NewWorkRequestInfo

    init(_ info: NewWorkRequestInfo) {
        self.info = info
    }

    /// A lead plus `children` implementers running `childPreset` (the
    /// lead's when `nil`), in `folder`, without a template or brief.
    public static func quick(
        named name: String,
        lead: String,
        children: Int = 0,
        childPreset: String? = nil,
        folder: String
    ) -> NewWorkRequest {
        NewWorkRequest(
            NewWorkRequestInfo(
                name: name,
                folder: folder,
                template: nil,
                kind: nil,
                lead: lead,
                children: (0..<max(0, children)).map { _ in
                    ChildRequestInfo(preset: childPreset ?? lead, role: .implementer)
                },
                readOnly: false,
                permission: nil,
                answers: [],
                voice: []
            )
        )
    }
}

/// What the new-work screen wants the app to do.
public enum NewWorkEvent: Equatable, Sendable {
    case start(NewWorkRequest)
    /// Reopen saved work.
    case reopen(WorkspaceID)
    /// Back to running work.
    case close
}

/// The welcome screen, template picker and new-work form. It lives in Rust
/// and is drawn by ``TerminalChrome``; feed it the chrome terminal's bytes.
public final class NewWorkScreen: Sendable {
    let flow: NewWorkFlow

    init(_ flow: NewWorkFlow) {
        self.flow = flow
    }

    /// Applies one chunk of terminal input.
    public func feed(_ input: Data) -> [NewWorkEvent] {
        flow.feed(input: input).map(NewWorkEvent.init)
    }

    /// Puts a voice note into the focused brief question, or adds it as a
    /// free note.
    public func add(_ clip: VoiceClip) {
        flow.addVoice(clip: VoiceClipInfo(clip, question: nil))
    }

    /// A one-line message, e.g. why starting failed.
    public func setNotice(_ notice: String?) {
        flow.setNotice(notice: notice)
    }

    /// "● recording", "transcribing …", or nothing.
    public func setVoiceStatus(_ status: String?) {
        flow.setVoiceStatus(status: status)
    }
}

// MARK: - Conversions from the generated FFI types

extension NewWorkEvent {
    init(_ event: FlowEventInfo) {
        switch event {
        case .start(let request): self = .start(NewWorkRequest(request))
        case .reopen(let workID): self = .reopen(workID)
        case .close: self = .close
        }
    }
}

extension VoiceClipInfo {
    init(_ clip: VoiceClip, question: String?) {
        self.init(
            recording: clip.recording,
            durationMs: clip.durationMilliseconds,
            transcript: clip.transcript,
            question: question,
            provider: clip.provider,
            language: clip.language
        )
    }
}
