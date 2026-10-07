# Swift practices for Makima

Research date: 2026-10-06

How to write Makima's Swift code: the macOS AppKit/SwiftUI shell that hosts libghostty terminals over the Rust core. Read alongside `rust-swift-practices.md` and `swift-performance.md`.

## Toolchain and project setup

- **Swift version.** Swift 6.4 is the current release (Sept 2026); this machine has **6.3.3**. libghostty-spm needs 6.2 or newer, so target `swift-tools-version: 6.2` and Swift 6 language mode, and upgrade to 6.4 when convenient. 6.4 adds Subprocess 1.0, `withTaskCancellationShield`, async calls in `defer`, and `UniqueArray`/`UniqueBox`.
- **Use SwiftPM as the source of truth,** with multiple targets that each have one job:
  - `MakimaApp`: the executable, with the AppKit app delegate and window setup.
  - `MakimaUI`: views, view models and the sidebar.
  - `MakimaTerminal`: the libghostty integration layer (wraps `GhosttyTerminal`).
  - `MakimaCore`: the hand-written Swift API over the Rust core: domain types, `CoreClient` (behind the `CoreService` protocol), `AppConfig`, `DesignTokens`, and the `TerminalChrome` facade.
  - `MakimaCoreBindings`: the UniFFI-generated Swift bindings (`Generated/`), plus the typed IDs they use.
  - `MakimaCoreFFI`: the generated C header and modulemap.
- **Platform:** `.macOS(.v14)` or later. macOS 14 brings `@Observable`, and libghostty-spm only needs 13.
- **Enable Swift 6.2 approachable concurrency in every target:**
  ```swift
  let swiftSettings: [SwiftSetting] = [
      .defaultIsolation(MainActor.self),                       // SE-0466 — UI targets only
      .enableUpcomingFeature("NonisolatedNonsendingByDefault"), // SE-0461
      .enableUpcomingFeature("InferIsolatedConformances"),
      .enableUpcomingFeature("MemberImportVisibility"),
      .enableUpcomingFeature("ExistentialAny"),
  ]
  ```
  Use `.defaultIsolation(MainActor.self)` for UI and app targets. Leave it off for `MakimaCore` and `MakimaCoreBindings`, which stay `nonisolated` and `Sendable`.
- **Exception: `MakimaCoreBindings`.** Generated code doesn't compile with `ExistentialAny` or `MemberImportVisibility`, so that target enables only the concurrency features. Nothing hand-written lives there except `IDs.swift`, which the generated code needs; `MakimaCore` follows this guide in full.
- **Who imports the bindings.** Only the chrome: the files that build `ChromeState`, drive `KeymapEngine`, `Picker` and `TalkEditor`, and map menu items to `KeyAction` (`TUIController`, `TUIViewController`, `AppDelegate`) import `MakimaCoreBindings`, because those are presentation types made for the host. Everything else (work, agents, config, design tokens, voice settings) goes through `MakimaCore`'s Swift types.
- **Formatting and linting:** `swift-format` (ships with the toolchain; `swift format`) for formatting, plus SwiftLint if you want extra rules. `just lint` runs it; there's no CI yet, so run it before pushing.
- **Warnings are errors** under `just lint` (`MAKIMA_STRICT=1` turns on `-warnings-as-errors` for Makima's own targets), and in CI once there is one.

## Types: struct, enum, class, actor

- **Use `struct` and `enum` by default.** Value types copy cheaply (collections are copy-on-write), compose well, and a struct with only `let` Sendable fields is `Sendable` for free.
- **Use `enum` for state.** Put data in associated values, not a status string plus optional fields:
  ```swift
  enum AgentStatus: Sendable, Equatable {
      case starting, working, waiting
      case inputRequired(prompt: String)
      case failed(reason: String)
      case completed
  }
  ```
- **Use `class` only when identity matters:**
  - AppKit types (`NSView`, `NSViewController`, `NSWindowController`);
  - `@Observable` view models;
  - UniFFI objects;
  - anything with a lifecycle such as a terminal session.

  Make classes `final` by default.
- **Use `actor` for shared mutable state touched from several tasks**, such as a registry of agent sessions fed by Rust callbacks. Don't make actors for things that already live on the main actor.
- **Noncopyable types (`~Copyable`)** suit unique resources like file descriptors or PTY handles. A noncopyable struct gives deterministic `deinit` without a class or reference counting.
- **Give IDs their own types:**
  ```swift
  struct WorkID: Hashable, Sendable, Codable { let rawValue: UUID }
  struct AgentID: Hashable, Sendable, Codable { let rawValue: UUID }
  ```
  A `WorkID` can't be passed where an `AgentID` is expected. Use `Tagged`-style generics if you have many of them.
- **Access control:** `private` by default, `internal` for in-module use, and `public` only on the API a target deliberately exposes. Mark modules' public types `Sendable` explicitly.
- **Prefer `let` over `var`, and immutable models over mutable ones.** Mutate through methods that keep invariants.
- **Follow the [Swift API Design Guidelines](https://www.swift.org/documentation/api-design-guidelines/) for naming:**
  - name for clarity at the call site (`agents.remove(at: index)`);
  - name side-effect-free functions as nouns (`distance(to:)`) and mutating ones as verbs (`sort()`/`sorted()`);
  - argument labels form grammatical phrases.

## Protocols and generics

- **Protocols describe capabilities you swap,** like Rust traits. Don't create one for a single implementation; wait for a second implementation or a test double.
  - Good fits in Makima: `AgentEventSink` (the Swift side of a UniFFI callback trait), `TerminalHost` (live vs preview), `Clock`.
- **Prefer composition over inheritance.** Combine small protocols (`Identifiable & Sendable`) rather than building class hierarchies.
- **Generics or existentials?**
  - Use generics or `some P` when the concrete type is fixed at the call site. It's faster (specialized, no boxing) and gives better type checking.
  - Use `any P` (explicit with `ExistentialAny` on) when different concrete types must live together in one collection, such as a heterogeneous list of adapters.
- **Use protocol extensions for shared default behaviour.** Don't put stored state logic in them.
- **Use primary associated types** (`protocol Store<Item>`) so callers can write `some Store<Work>`.

## Concurrency (Swift 6.2+)

- **Default to the main actor in UI code.** With `defaultIsolation(MainActor.self)`, all UI-target code runs on main. You prove where you *leave* main, not where you stay.
- **`nonisolated`** is for pure functions, constants, statics, `Codable`/`Hashable` conformances, and model types with no UI state.
- **`@concurrent`** marks functions that deliberately run off the main actor: parsing big files, hashing, image work. It visibly marks the few places where code runs off the main actor.
- **Nonisolated async functions now run on the caller's actor by default (SE-0461).** Don't add `Task.detached` to "get off main"; use `@concurrent`.
- **Prefer structured concurrency:** `async let`, `withTaskGroup` / `withThrowingTaskGroup` and `withDiscardingTaskGroup`. Keep unstructured `Task { }` to bridging points (AppKit callbacks, Rust callbacks), and store its handle so you can cancel it.
- **Handle cancellation:**
  - check `Task.isCancelled` / `try Task.checkCancellation()` in loops;
  - cancel tasks owned by a view or window when it closes;
  - Swift 6.4's `withTaskCancellationShield` makes cleanup run even when cancelled.
- **Bridge Rust callbacks:** the Swift type implementing a UniFFI callback protocol must be `Sendable`. Forward into the app with `AsyncStream`:
  ```swift
  // Sendable without @unchecked: its only stored property, the continuation, is Sendable.
  final class AgentEventBridge: AgentEventSink, Sendable {
      let continuation: AsyncStream<AgentEvent>.Continuation
      func onEvent(event: AgentEvent) { continuation.yield(event) }  // called from Rust threads
  }
  // UI side:
  for await event in stream { model.apply(event) }  // runs on MainActor
  ```
- **Avoid exported UniFFI `async fn`** until its Swift 6 `Sendable` gaps close (see `rust-swift-practices.md`).
- **`@unchecked Sendable` and `nonisolated(unsafe)` are last resorts.** Each use needs a comment saying why it's safe.
- **No `DispatchQueue` in new code** except where AppKit or C APIs need it. Use `MainActor.run` and actors instead of `DispatchQueue.main.async`.

## Errors

- **Use typed domain errors:** enums conforming to `Error`, plus `LocalizedError` for anything shown to users.
  ```swift
  enum VaultError: Error, LocalizedError {
      case notFound(WorkID)
      case invalidFrontmatter(path: String, underlying: any Error)
      var errorDescription: String? { … }
  }
  ```
- **Typed throws (`throws(VaultError)`)** fit small, closed error sets inside a module. Plain `throws` is more flexible across module boundaries, so default to it for public API.
- **UniFFI-generated Rust errors arrive as Swift error enums.** Map them to UI messages at the edge; don't leak FFI types into views.
- **Never `try!` or force-unwrap on external data.** Keep `!` for true programmer invariants, and write the invariant into the crash message:
  ```swift
  guard let x else { preconditionFailure("why this can't happen") }
  ```
- **Add context while unwinding** by wrapping the error in a case that names the operation (`case loadingWork(id: WorkID, underlying: any Error)`), which plays the role of `anyhow` context in Rust.

## App architecture

- **Observation:** use `@Observable` (macOS 14+) for view models, not `ObservableObject`/`@Published`. It's simpler and only re-renders on properties actually read.
- **Hybrid AppKit + SwiftUI:**
  - AppKit owns windows, menus, first responder and the libghostty `AppTerminalView`, because terminals need precise keyboard and focus control.
  - SwiftUI suits the sidebar, inspectors and settings, embedded with `NSHostingView`.
- **One-way data flow:** Rust core → event stream → `@Observable` stores on main → views. User intents go back as method calls on the core. Views never talk to Rust directly.
- **Dependency injection:** build services once in `AppDelegate`, pass them down through initializers or the SwiftUI `Environment`, and avoid singletons except for truly global, immutable things.
- **The terminal stays authoritative:** don't mirror terminal contents into Swift state. libghostty owns the grid; Swift owns layout, focus and lifecycle.

## Recommended packages

| Need | Package | Notes |
|---|---|---|
| Terminal | `Lakr233/libghostty-spm` (`GhosttyTerminal`) | `from: "2.2.2026100501"` (see `Package.swift`); `.exec` backend for real shells |
| Process spawning | `swiftlang/swift-subprocess` | 1.0 in Swift 6.4; async, concurrency-native. Use `Foundation.Process` only if you must |
| CLI | `apple/swift-argument-parser` | 1.8+ requires Swift 6; for any `makima` Swift CLI tool |
| Logging | `os.Logger` (built-in) | Unified logging, privacy redaction, Console.app/Instruments integration. Add `apple/swift-log` only for libraries that must stay logger-agnostic |
| Collections | `apple/swift-collections` | `OrderedDictionary`, `Deque`, `Heap`, `TreeDictionary` |
| Async sequences | `apple/swift-async-algorithms` | `debounce`, `throttle`, `merge`, `combineLatest`, `AsyncChannel` |
| Algorithms | `apple/swift-algorithms` | Chunking, windows, uniquing, partitioning |
| System calls | `apple/swift-system` | Typed `FileDescriptor`, `FilePath` |
| Testing | **Swift Testing** (`import Testing`) | The recommended framework; XCTest is legacy. `@Test`, `#expect`, `#require`, parameterized tests |
| Snapshots | `pointfreeco/swift-snapshot-testing` | UI and model snapshots |
| Dependencies/DI | `pointfreeco/swift-dependencies` | Optional; plain initializer injection is enough to start |
| Benchmarks | `ordo-one/package-benchmark` | See `swift-performance.md` |
| Format/lint | `swift-format`, SwiftLint | |

Keep the dependency count small. Every package adds to build time and supply-chain exposure, and the toolchain and Apple packages cover most needs.

## Testing

- Use Swift Testing for all new tests:
  - `@Test` functions;
  - `@Suite` structs (a fresh instance per test, so there's no shared-state leakage);
  - `#expect` / `try #require`;
  - parameterized `@Test(arguments:)`.
- Keep test logic in `MakimaCore` and `MakimaUI` view models, not in the executable target.
- Inject the Rust core behind a protocol so UI and view-model tests can run against a fake core.
- Async tests are just `async` test functions. Avoid sleeps; inject a clock instead.

## Quick checklist

- [ ] Structs and enums by default; `final class` only for identity; actors for shared mutable state
- [ ] Wrapper types for IDs, enums for state, `private` by default
- [ ] Default MainActor isolation in UI targets; `nonisolated` core; `@concurrent` for background work
- [ ] Structured concurrency; stored, cancelled `Task` handles; no `DispatchQueue` in new code
- [ ] No `try!` or force-unwraps on external data; typed domain errors
- [ ] `@Observable` view models; one-way data flow from the Rust core
- [ ] Swift Testing; `swift-format` clean; `just lint` passes (warnings are errors)

## Sources

- [Swift 6.4 released](https://www.swift.org/blog/swift-6.4-released/)
- [Swift 6.2 released](https://www.swift.org/blog/swift-6.2-released/)
- [Swift 6.4: Subprocess 1.0, interop, Wasm (InfoQ)](https://www.infoq.com/news/2026/09/swift-6-4-released/)
- [What's new in Swift, WWDC26](https://developer.apple.com/videos/play/wwdc2026/262/)
- [What's new in Swift (2026)](https://blakecrosley.com/blog/whats-new-swift-2026)
- [Swift API Design Guidelines](https://www.swift.org/documentation/api-design-guidelines/)
- [Swift: best practices](https://llmbestpractices.com/coding/swift)
- [Best practices for building Swift classes (AppCoda)](https://www.appcoda.com/swift-class-best-practice/)
- [Swift protocols (BairesDev)](https://www.bairesdev.com/blog/swift-protocols/)
- [Swift 6.2 approachable concurrency: complete guide](https://swiftcrafted.dev/article/swift-6-2-approachable-concurrency-complete-guide)
- [Swift 6.2 concurrency in practice](https://blakecrosley.com/blog/swift-6-2-concurrency-in-practice)
- [Swift 6.2 concurrency changes (SwiftLee)](https://www.avanderlee.com/concurrency/swift-6-2-concurrency-changes/)
- [Should you opt in to Swift 6.2's MainActor isolation? (Donny Wals)](https://www.donnywals.com/should-you-opt-in-to-swift-6-2s-main-actor-isolation/)
- [Exploring concurrency changes in Swift 6.2 (Donny Wals)](https://www.donnywals.com/exploring-concurrency-changes-in-swift-6-2/)
- [Swift 6.2 default isolation and @concurrent](https://newsletter.mobileengineer.io/p/swift-62-default-concurrency-isolation)
- [swift-subprocess](https://github.com/swiftlang/swift-subprocess)
- [swift-argument-parser](https://github.com/apple/swift-argument-parser)
- [swift-async-algorithms](https://swiftpackageregistry.com/apple/swift-async-algorithms)
- [Swift logging (NSHipster)](https://nshipster.com/swift-log/)
- [swift-format updates](https://releasebot.io/updates/swift/swift-format)
