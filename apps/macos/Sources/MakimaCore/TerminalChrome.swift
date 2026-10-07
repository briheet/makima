public import MakimaCoreBindings

/// The terminal chrome from `makima-tui`: rendering, `@` routing, the
/// `makima>` command language, the key maps and pane neighbours.
///
/// The chrome types (`ChromeState`, `ChromeFrame`, `KeymapEngine`,
/// `KeyAction`, `Picker`, …) are presentation DTOs and engines designed
/// for the host, and are used as they are: the chrome files in MakimaUI
/// import `MakimaCoreBindings` for them, and nothing else does. This facade
/// turns `InputError` into a message. Domain data (work, agents, config,
/// voice) never crosses as generated records: see ``CoreClient`` and
/// ``AppConfig``.
public enum TerminalChrome {
    /// Bytes and pane rects for a `columns` × `rows` chrome terminal. `keys`
    /// sets the mode badge and shows a pending minor mode's keys; a `screen`
    /// fills the body and a `picker` is drawn over it.
    public static func render(
        _ state: ChromeState,
        keys: KeymapEngine,
        screen: NewWorkScreen?,
        picker: Picker?,
        columns: UInt16,
        rows: UInt16
    ) -> ChromeFrame {
        renderChrome(state: state, keys: keys, flow: screen?.flow, picker: picker, columns: columns, rows: rows)
    }

    /// Who `input` (optionally starting with `@target`) is for.
    public static func route(
        _ input: String,
        defaultTarget: TalkTarget,
        in workspace: Workspace
    ) -> Result<RoutedTalk, InputMistake> {
        let recipient = { (agent: Agent) in TalkRecipient(id: agent.id, name: agent.name) }
        return catching {
            try routeTalk(
                input: input,
                defaultTarget: defaultTarget,
                lead: workspace.lead.map(recipient),
                children: workspace.children.map(recipient)
            )
        }
    }

    /// Parses a `makima>` line.
    public static func parse(command line: String) -> Result<MakimaCommand, InputMistake> {
        catching { try parseMakimaCommand(line: line) }
    }

    /// The pane next to `agent` in `direction`, from the last frame.
    public static func neighbor(of agent: AgentID, _ direction: FocusDirection, in panes: [ChromePaneRect]) -> AgentID?
    {
        neighborPane(panes: panes, from: agent, direction: direction)
    }

    private static func catching<T>(_ body: () throws -> T) -> Result<T, InputMistake> {
        do {
            return .success(try body())
        } catch let error as InputError {
            return .failure(InputMistake(message: inputErrorMessage(error: error)))
        } catch {
            return .failure(InputMistake(message: String(describing: error)))
        }
    }
}

/// Something typed at the talk line or `makima>` prompt that can't be done.
public struct InputMistake: Error, Equatable, Sendable {
    public let message: String
}
