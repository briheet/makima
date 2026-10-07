import AppKit
import MakimaCore
// The menu bar runs key-map actions (`KeyAction`), a chrome type.
import MakimaCoreBindings
import MakimaUI

/// The app's entry point and composition root: shows the window, then builds
/// the core and model off the main thread and hands them to the window.
@main
final class AppDelegate: NSObject, NSApplicationDelegate {
    /// The target defaults to the main actor, so this runs on it.
    static func main() {
        let app = NSApplication.shared
        // `NSApplication.delegate` is weak; this local outlives `run()`.
        let delegate = AppDelegate()
        app.delegate = delegate
        app.setActivationPolicy(.regular)
        app.run()
    }

    private var window: NSWindow?
    private var tuiController: TUIViewController?
    private var backgroundLoop: ObservationLoop?
    private var model: AppModel?
    private var appearance: AppearanceStore?
    private var eventStream: CoreEventStream?
    private var launchTask: Task<Void, Never>?
    private var eventTask: Task<Void, Never>?
    private var scriptTask: Task<Void, Never>?

    /// Shows the window straight away, in the configured theme, then opens
    /// the core (config, store, saved work) off the main thread and fills
    /// the window when it's ready.
    func applicationDidFinishLaunching(_: Notification) {
        let options = LaunchOptions(arguments: CommandLine.arguments)
        // The window's look comes from the config: one small file.
        let loaded = AppConfig.load(path: options.configPath)
        let appearance = AppearanceStore(loaded: loaded)
        self.appearance = appearance
        showWindow(appearance: appearance)
        NSApp.activate()

        let events = CoreEventStream()
        eventStream = events
        launchTask = Task { [weak self] in
            let core = await Self.openCore(events: events, configPath: loaded.path)
            self?.coreOpened(core, events: events, config: loaded.config, options: options)
        }
    }

    /// The core reads the config again and opens the store's databases.
    @concurrent
    private static func openCore(events: CoreEventStream, configPath: String?) async -> CoreClient {
        CoreClient(events: events, configPath: configPath, storage: .standard)
    }

    /// Fills the window and menu bar, and runs the launch options.
    private func coreOpened(_ core: CoreClient, events: CoreEventStream, config: AppConfig, options: LaunchOptions) {
        guard let appearance, let window else { return }
        let model = AppModel(core: core)
        self.model = model
        eventTask = Task { await model.run(changes: events.changes) }

        let tui = TUIViewController(model: model, appearance: appearance)
        window.contentViewController = tui
        window.setContentSize(Self.windowSize)
        tuiController = tui
        NSApp.mainMenu = MainMenu.make(target: self, presets: model.presets, tools: config.tools, keys: config.keys)
        options.apply(to: model)
        scriptTask = Task { [weak self] in
            // Give the agents' shells a moment to start before talking.
            if let line = options.say {
                try? await Task.sleep(for: .seconds(2))
                self?.tuiController?.say(line)
            }
            if let tool = options.tool {
                try? await Task.sleep(for: .seconds(1))
                self?.tuiController?.openTool(id: tool)
            }
            if let path = options.dumpChromePath {
                try? await Task.sleep(for: .seconds(2))
                let text = self?.tuiController?.chromeText() ?? "(no chrome)"
                Self.write(text, toFile: path)
            }
        }
    }

    func applicationWillTerminate(_: Notification) {
        eventStream?.finish()
        for task in [launchTask, eventTask, scriptTask] { task?.cancel() }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_: NSApplication) -> Bool {
        true
    }

    func applicationSupportsSecureRestorableState(_: NSApplication) -> Bool {
        true
    }

    /// Writes the `--dump-chrome` output; a failure is reported on stderr,
    /// since whoever asked for it is reading the terminal, not the window.
    private static func write(_ text: String, toFile path: String) {
        do {
            try text.write(toFile: path, atomically: true, encoding: .utf8)
        } catch {
            let message = "makima: couldn't write --dump-chrome to \(path): \(error.localizedDescription)\n"
            FileHandle.standardError.write(Data(message.utf8))
        }
    }

    // MARK: - Menu actions (each mirrors a key-map action)

    @objc func newWork(_: Any?) { tuiController?.perform(.newWork) }
    @objc func pickWork(_: Any?) { tuiController?.perform(.workPicker) }
    @objc func nextWork(_: Any?) { tuiController?.perform(.nextWork) }
    @objc func previousWork(_: Any?) { tuiController?.perform(.previousWork) }
    @objc func newChild(_: Any?) { tuiController?.perform(.newChild) }
    @objc func talkToAll(_: Any?) { tuiController?.perform(.talkToAll) }
    @objc func command(_: Any?) { tuiController?.perform(.command) }
    @objc func closeAgent(_: Any?) { tuiController?.perform(.close) }
    @objc func toggleZoom(_: Any?) { tuiController?.perform(.zoom) }
    @objc func nextAgent(_: Any?) { tuiController?.perform(.nextAgent) }
    @objc func previousAgent(_: Any?) { tuiController?.perform(.previousAgent) }
    @objc func showKeys(_: Any?) { tuiController?.perform(.help) }

    /// The tool's index travels in the menu item's `representedObject`.
    @objc func openTool(_ sender: NSMenuItem) {
        guard let index = sender.representedObject as? Int, let index = UInt16(exactly: index) else { return }
        tuiController?.perform(.tool(index: index))
    }

    /// The preset id travels in the menu item's `representedObject`.
    @objc func newPresetAgent(_ sender: NSMenuItem) {
        guard let presetID = sender.representedObject as? String else { return }
        tuiController?.addAgent(presetID: presetID)
    }

    /// Opens the config file in the default editor, creating it from the
    /// commented defaults if it doesn't exist yet.
    @objc func openConfig(_: Any?) {
        guard let path = appearance?.configPath else { return }
        let url = URL(fileURLWithPath: path)
        if !FileManager.default.fileExists(atPath: path) {
            do {
                try FileManager.default.createDirectory(
                    at: url.deletingLastPathComponent(),
                    withIntermediateDirectories: true
                )
                try AppConfig.defaultText.write(to: url, atomically: true, encoding: .utf8)
            } catch {
                NSAlert(error: error).runModal()
                return
            }
        }
        NSWorkspace.shared.open(url)
    }

    @objc func zoomIn(_: Any?) { appearance?.zoom(by: 1) }
    @objc func zoomOut(_: Any?) { appearance?.zoom(by: -1) }
    @objc func resetZoom(_: Any?) { appearance?.resetZoom() }

    @objc func reloadConfig(_: Any?) {
        appearance?.reload()
    }

    // MARK: - Window

    private static let windowSize = NSSize(width: 1280, height: 800)

    /// The window, empty in the theme's background until the core is open.
    private func showWindow(appearance: AppearanceStore) {
        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: Self.windowSize),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = "Makima"
        window.titleVisibility = .hidden
        window.titlebarAppearsTransparent = true
        window.contentMinSize = NSSize(width: 700, height: 400)
        window.center()
        window.makeKeyAndOrderFront(nil)
        backgroundLoop = appearance.observeBackground { [weak window] background in
            window?.backgroundColor = background
        }
        self.window = window
    }
}
