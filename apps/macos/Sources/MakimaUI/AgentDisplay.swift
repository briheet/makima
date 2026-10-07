import MakimaCore

extension Agent.Status {
    var label: String {
        switch self {
        case .starting: "Starting"
        case .idle: "Idle"
        case .working: "Working"
        case .inputRequired: "Needs input"
        case .completed: "Completed"
        case .failed(let reason): "Failed: \(reason)"
        case .interrupted: "Interrupted"
        }
    }
}
