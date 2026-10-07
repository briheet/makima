import Foundation
import MakimaCore
import MakimaCoreBindings

extension TUIController {
    /// Everything the renderer draws, from the model and this controller.
    func chromeState(panes displays: [AgentID: PaneDisplay]) -> ChromeState {
        _ = (redraws, revision)  // read so a bump re-renders
        let workspace = model.selectedWorkspace
        let agents = workspace?.agents ?? []
        let panes = agents.enumerated().map { index, agent in
            let display = displays[agent.id] ?? PaneDisplay()
            return ChromePane(
                id: agent.id,
                name: agent.name,
                role: index == 0 ? .lead : .child(number: UInt16(index)),
                status: display.isClosed && agent.status.isFinished ? .exited : ChromePaneStatus(agent.status),
                detail: agent.status == .working ? display.title : "",
                focused: agent.id == model.selectedAgentID && !isTyping && shownTool == nil,
                unguarded: agent.permission == .bypass
            )
        }
        var counts = ChromeCounts(working: 0, waiting: 0, done: 0, failed: 0)
        for agent in agents {
            switch agent.status {
            case .working: counts.working += 1
            case .inputRequired: counts.waiting += 1
            case .completed: counts.done += 1
            case .failed, .interrupted: counts.failed += 1
            case .starting, .idle: break
            }
        }
        let tabs = model.workspaces.map { work in
            ChromeTab(
                label: work.name,
                active: work.id == workspace?.id && mode != .newWork,
                attention: work.agents.contains {
                    if case .inputRequired = $0.status { true } else { false }
                }
            )
        }
        return ChromeState(
            tabs: tabs,
            panes: panes,
            zoomed: model.zoomedAgentID,
            mode: chromeMode,
            talk: ChromeTalkLine(prompt: prompt, text: lineText, cursor: lineCursor),
            counts: counts,
            workName: mode == .newWork ? nil : workspace?.name,
            clock: clock,
            showHelp: showHelp,
            confirm: { if case .confirm(let c) = mode { c.question } else { nil } }(),
            notice: mode == .newWork ? nil : (voiceStatus ?? notice ?? model.lastError?.localizedDescription),
            float: mode == .newWork ? nil : chromeFloat,
            leader: leaderLabel
        )
    }

    var chromeMode: ChromeMode {
        switch mode {
        case .pane: .pane
        case .keys: .keys
        case .picker: .picker
        case .newWork: .newWork
        case .talk: .talk
        case .command: .command
        case .confirm: .confirm
        case .tool: .tool
        }
    }

    var prompt: String {
        switch mode {
        case .talk(.lead): "@lead ▸ "
        case .talk(.all): "@all ▸ "
        case .talk(.children): "@children ▸ "
        case .command: "makima> "
        default: ""
        }
    }
}

extension ChromePaneStatus {
    init(_ status: Agent.Status) {
        switch status {
        case .starting, .idle: self = .starting
        case .working: self = .working
        case .inputRequired: self = .needsInput
        case .completed: self = .done
        case .failed, .interrupted: self = .failed
        }
    }
}
