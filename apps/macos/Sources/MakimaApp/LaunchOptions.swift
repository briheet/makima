import Foundation
import MakimaUI

/// Starts work straight from the command line, for scripting and testing:
///
///     MakimaApp --work "Fix CI" --preset claude --children 3
///     MakimaApp --work "Build" --command "make test"
///     MakimaApp --config /path/to/config.toml
///     MakimaApp --work x --say "@all run the tests"   # talk once the agents start
///     MakimaApp --dump-chrome /tmp/chrome.txt   # write the chrome's text after 3s
///     MakimaApp --work x --tool git             # open a tool once the work starts
struct LaunchOptions {
    var configPath: String?
    var dumpChromePath: String?
    var say: String?
    var tool: String?
    var work: String?
    var preset = "shell"
    var children = 0
    var command: String?

    init(arguments: [String]) {
        var iterator = arguments.dropFirst().makeIterator()
        while let flag = iterator.next() {
            switch flag {
            case "--work": work = iterator.next()
            case "--preset": preset = iterator.next() ?? preset
            case "--children": children = iterator.next().flatMap(Int.init) ?? children
            case "--command": command = iterator.next()
            case "--config": configPath = iterator.next()
            case "--dump-chrome": dumpChromePath = iterator.next()
            case "--say": say = iterator.next()
            case "--tool": tool = iterator.next()
            default: continue
            }
        }
    }

    func apply(to model: AppModel) {
        guard let work else { return }
        model.startWork(
            named: work,
            lead: preset,
            children: children,
            folder: FileManager.default.currentDirectoryPath
        )
        if let command { model.addCustomAgent(command: command) }
    }
}
