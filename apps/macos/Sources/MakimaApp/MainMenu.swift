import AppKit
import MakimaCore
import MakimaUI

/// The menu bar. Every Work item mirrors a key-map action, so the TUI is
/// fully usable with either; titles show the keys for the active style.
enum MainMenu {
    static func make(target: AppDelegate, presets: [AgentPreset], tools: [AppConfig.Tool], keys: AppConfig.Keys)
        -> NSMenu
    {
        let main = NSMenu()
        let keys = Hints(style: keys.style, leader: keys.leader.label)
        main.addItem(submenu(appMenu(target: target)))
        main.addItem(submenu(workMenu(target: target, presets: presets, keys: keys)))
        main.addItem(submenu(toolsMenu(target: target, tools: tools, keys: keys)))
        main.addItem(submenu(editMenu()))
        main.addItem(submenu(windowMenu()))
        return main
    }

    /// The keys for each action, after the leader key.
    private struct Hints {
        let style: AppConfig.Keys.Style
        /// The key into Makima's keys, e.g. `^Space`.
        let leader: String

        /// `nil` where the style has no binding.
        func callAsFunction(helix: String?, tmux: String?) -> String {
            (style == .tmux ? tmux : helix).map { "\(leader) \($0)" } ?? ""
        }
    }

    private static func submenu(_ menu: NSMenu) -> NSMenuItem {
        let item = NSMenuItem()
        item.submenu = menu
        return item
    }

    private static func appMenu(target: AppDelegate) -> NSMenu {
        let menu = NSMenu(title: "Makima")
        menu.addItem(item("Open Config…", #selector(AppDelegate.openConfig(_:)), ",", target: target))
        menu.addItem(
            item("Reload Config", #selector(AppDelegate.reloadConfig(_:)), ",", [.command, .shift], target: target))
        menu.addItem(.separator())
        menu.addItem(withTitle: "Hide Makima", action: #selector(NSApplication.hide(_:)), keyEquivalent: "h")
        menu.addItem(.separator())
        menu.addItem(withTitle: "Quit Makima", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        return menu
    }

    private static func workMenu(target: AppDelegate, presets: [AgentPreset], keys: Hints) -> NSMenu {
        let menu = NSMenu(title: "Work")
        func add(
            _ title: String, _ hint: String, _ action: Selector, _ key: String,
            _ modifiers: NSEvent.ModifierFlags = [.command]
        ) {
            menu.addItem(
                item(
                    "\(title.padding(toLength: 22, withPad: " ", startingAt: 0)) \(hint)", action, key, modifiers,
                    target: target))
        }
        add("New Work", keys(helix: "space n", tmux: "n"), #selector(AppDelegate.newWork(_:)), "n")
        add("Pick Work", keys(helix: "space b", tmux: "w"), #selector(AppDelegate.pickWork(_:)), "p")
        add("Next Work", keys(helix: "]", tmux: "]"), #selector(AppDelegate.nextWork(_:)), "}", [.command, .shift])
        add(
            "Previous Work", keys(helix: "[", tmux: "["), #selector(AppDelegate.previousWork(_:)), "{",
            [.command, .shift])
        menu.addItem(.separator())
        add("New Child", keys(helix: "space c", tmux: "c"), #selector(AppDelegate.newChild(_:)), "t")
        let agents = NSMenuItem(title: "New Agent", action: nil, keyEquivalent: "")
        agents.submenu = NSMenu(title: "New Agent")
        for preset in presets {
            let new = item(preset.label, #selector(AppDelegate.newPresetAgent(_:)), "", target: target)
            new.representedObject = preset.id
            agents.submenu?.addItem(new)
        }
        menu.addItem(agents)
        menu.addItem(.separator())
        add(
            "Message Everyone", keys(helix: "space a", tmux: "a"), #selector(AppDelegate.talkToAll(_:)), "l",
            [.command, .shift])
        add("Command", keys(helix: ":", tmux: ":"), #selector(AppDelegate.command(_:)), ";")
        menu.addItem(.separator())
        add("Zoom Pane", keys(helix: "space z", tmux: "z"), #selector(AppDelegate.toggleZoom(_:)), "\r")
        // Moving between agents is Ctrl+h/j/k/l while typing.
        add("Next Agent", keys(helix: nil, tmux: nil), #selector(AppDelegate.nextAgent(_:)), "]")
        add("Previous Agent", keys(helix: nil, tmux: nil), #selector(AppDelegate.previousAgent(_:)), "[")
        menu.addItem(.separator())
        add("Close Agent", keys(helix: "space x", tmux: "x"), #selector(AppDelegate.closeAgent(_:)), "w")
        menu.addItem(.separator())
        add("Keys", keys(helix: "?", tmux: "?"), #selector(AppDelegate.showKeys(_:)), "/")
        return menu
    }

    /// One item per tool; each opens or hides it over the work on screen.
    private static func toolsMenu(target: AppDelegate, tools: [AppConfig.Tool], keys: Hints) -> NSMenu {
        let menu = NSMenu(title: "Tools")
        for (index, tool) in tools.enumerated() {
            let hint = tool.key.map { keys(helix: "space \($0)", tmux: $0) } ?? ""
            let title = "\(tool.label.padding(toLength: 22, withPad: " ", startingAt: 0)) \(hint)"
            let entry = item(title, #selector(AppDelegate.openTool(_:)), "", target: target)
            entry.representedObject = index
            menu.addItem(entry)
        }
        return menu
    }

    private static func item(
        _ title: String,
        _ action: Selector,
        _ key: String,
        _ modifiers: NSEvent.ModifierFlags = [.command],
        target: AppDelegate
    ) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: action, keyEquivalent: key)
        item.keyEquivalentModifierMask = modifiers
        item.target = target
        return item
    }

    private static func editMenu() -> NSMenu {
        let menu = NSMenu(title: "Edit")
        menu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        menu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        menu.addItem(withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        return menu
    }

    private static func windowMenu() -> NSMenu {
        let menu = NSMenu(title: "Window")
        menu.addItem(withTitle: "Bigger", action: #selector(AppDelegate.zoomIn(_:)), keyEquivalent: "=")
        menu.addItem(withTitle: "Smaller", action: #selector(AppDelegate.zoomOut(_:)), keyEquivalent: "-")
        menu.addItem(withTitle: "Actual Size", action: #selector(AppDelegate.resetZoom(_:)), keyEquivalent: "0")
        menu.addItem(.separator())
        menu.addItem(withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")
        menu.addItem(withTitle: "Zoom", action: #selector(NSWindow.performZoom(_:)), keyEquivalent: "")
        NSApp.windowsMenu = menu
        return menu
    }
}
