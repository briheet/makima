# Rust core + Swift app: practices for Makima

Research date: 2026-10-06

How to build Makima's macOS desktop app as a Swift/AppKit shell with libghostty terminals, over a Rust core bound with UniFFI.

## Architecture

- **Keep the bridge narrow.** Rust owns shared state and session logic. Swift owns everything native: windows, views, input and libghostty. Swift calls a small, deliberate API and never reaches into Rust's internals.
- **Have one root object.** Expose a single long-lived `#[derive(uniffi::Object)]` (for example `MakimaCore`) behind an `Arc`. Swift holds one reference to it; Swift's ARC and Rust's `Drop` together manage its lifetime. Guard internal state with `Arc<Mutex/RwLock>` so Swift can call in from any thread.
- **Records for data, Objects for handles.**
  - `#[derive(uniffi::Record)]` is for plain values Swift reads directly, such as status or agent summaries. It becomes a Swift `struct`.
  - `#[derive(uniffi::Object)]` is for things with identity or a lifetime, such as sessions and workspaces. It becomes a Swift `class` with private fields, reached only through `#[uniffi::export]` methods.
- **Typed errors.** Derive `#[derive(uniffi::Error)]` on a `thiserror` enum. Swift gets real `throws` and an error enum it can `switch` over, not strings.
- **Keep hot paths off UniFFI.** UniFFI copies values across the boundary, so it suits control calls (sessions, events, status), not per-frame data. libghostty renders the terminals itself, so terminal bytes should never pass through Rust or UniFFI. If a hot data path ever appears, give it a raw C struct that Swift reads without copying, as phux does.
- **Rust owns its async runtime.** Rust runs its own tokio runtime and pushes events to Swift through a callback trait (`#[uniffi::export(with_foreign)]`). Swift should not drive Rust futures directly.

## Swift 6 concurrency

- UniFFI's Swift 6 support is only partial. Most generated code conforms to `Sendable`, but **exported `async fn` does not yet** (uniffi-rs #2274 / #2448). Prefer synchronous calls plus a callback interface over exported async functions for now.
- Swift types that implement a Rust callback trait must be `Sendable`. A `final class` that hops to the main actor with `Task { @MainActor in … }` covers this.
- In Swift 6.2 style, the main actor is the default isolation and you opt out explicitly:
  - mark pure helpers, constants and statics `nonisolated`;
  - mark deliberate background work `@concurrent`;
  - keep UI code on `@MainActor`.

## Build and packaging

- **Crate types:** `crate-type = ["staticlib", "lib"]`. `staticlib` is what Swift links against; `lib` is needed for Rust tests and the bindgen binary.
- **Pin the toolchain:** commit a `rust-toolchain.toml` that lists every target.
- **Pin the bindgen version:** run `uniffi-bindgen` from inside the workspace, as a small `[[bin]]` that depends on `uniffi` with the `cli` feature. This locks the generator to the same version as the `uniffi` crate. A separately installed `cargo-swift` embeds its own `uniffi_bindgen`, and a mismatch there breaks builds (example: marfa #1802, uniffi 0.32.2 vs cargo-swift's 0.31.1).
- **What gets generated:** a C header, a `module.modulemap` and a `.swift` file.
  - The header and modulemap go in an FFI (C/system) target (`MakimaCoreFFI`).
  - The `.swift` file goes in its own target that depends on the FFI target (`MakimaCoreBindings`).
  - A hand-written target (`MakimaCore`) wraps it in Swift types for the app; see `swift-practices.md` for which files may import the bindings directly.
- **Use scripts, not Xcode build phases:** drive the build from shell scripts or `just` recipes.
- **Release packaging (later):**
  1. Build each target.
  2. Merge architectures with `lipo`.
  3. Run `xcodebuild -create-xcframework`.
  4. Zip with `ditto`.
  5. Compute the checksum with `swift package compute-checksum`.
  6. Have CI write the tag and checksum into `Package.swift`.

  Teams usually keep a local/remote switch in `Package.swift`, because one manifest rarely serves both local development and release.

## Notes specific to this machine and repo

- **No full Xcode.** Only the Command Line Tools are installed, so `xcodebuild -create-xcframework` won't run. For local macOS development, skip the xcframework: link the Rust `.a` from SwiftPM through a modulemap target plus linker flags.
- **libghostty-spm** (`https://github.com/Lakr233/libghostty-spm`):
  - It needs Swift 6.2 or newer; the installed Swift 6.3.3 satisfies this.
  - Use `from: "1.5.2"` as the README says. The registry page's `0.1.0` line is out of date.
  - Products are `GhosttyKit` (the C API), `GhosttyTerminal` (AppKit/SwiftUI views), `GhosttyTheme` and `ShellCraftKit`.
  - Its `.exec` backend spawns a real shell and injects shell integration (OSC 133 prompt marks, OSC 7 working directory, title). Every agent therefore stays a real, interactive terminal, as `base_plan.md` requires.
- **Division of responsibility:**
  - Swift handles UI and terminals through libghostty.
  - The Rust core handles native shared logic: the workspace/agent model, the vault, and the connection to the Elixir orchestrator.
  - Elixir remains the control plane, as in `base_plan.md`.
- **Rust is optional for now.** If Swift could talk to Elixir directly, Rust would only earn its place for logic shared by several clients (CLI, headless, GUI).

## Sources

- [UniFFI Swift bindings overview](https://mozilla.github.io/uniffi-rs/latest/swift/overview.html)
- [UniFFI: Integrating with Xcode](https://mozilla.github.io/uniffi-rs/latest/swift/xcode.html)
- [uniffi-rs #2274: Swift 6 async data-race errors](https://github.com/mozilla/uniffi-rs/issues/2274)
- [marfa #1802: uniffi / cargo-swift version mismatch](https://github.com/withmarfa/marfa/issues/1802)
- [phux #824: UniFFI control plane + C RenderState data plane](https://github.com/no-phux/phux/issues/824)
- [Ferrostar: Rust on iOS, XCFramework & SwiftPM packaging](https://stadiamaps.com/blog/ferrostar-building-a-cross-platform-navigation-sdk-in-rust-part-2/)
- [Mozilla: Shipping Rust components as Swift packages](https://mozilla.github.io/application-services/book/design/swift-package-manager.html)
- [Zero-Trust iOS architecture: SwiftUI meets Rust (Part 2)](https://medium.com/@alfred.weirich/building-a-zero-trust-ios-architecture-swiftui-meets-rust-and-mls-part-2-3fd6a955745f)
- [Greenfield5 #15: UniFFI production bridge](https://github.com/anthracite-labs/Greenfield5/issues/15)
- [Bridging the gap between Swift & Rust](https://boehs.org/node/uniffi)
- [Swift 6.2 concurrency in practice](https://blakecrosley.com/blog/swift-6-2-concurrency-in-practice)
- [Sendable and @Sendable closures](https://www.avanderlee.com/swift/sendable-protocol-closures/)
- [swift-bridge](https://lib.rs/crates/swift-bridge)
- [libghostty-spm (GhosttyKit)](https://github.com/Lakr233/libghostty-spm)
- [awesome-libghostty](https://github.com/Uzaaft/awesome-libghostty)
- [Embedding libghostty in a Swift app: six months of lessons](https://dev.to/yuu1ch13/what-it-takes-to-embed-libghostty-in-a-swift-app-six-months-of-lessons-1m64)
