# Swift performance: benchmarking, profiling and tuning for Makima

Research date: 2026-10-06

How to measure and improve the speed of Makima's Swift app. Read alongside `swift-practices.md` and `rust-performance.md`.

## Ground rules

1. **Measure before optimizing,** and record a before/after number for every change.
2. **Measure release builds** (`swift build -c release`). Debug Swift is dramatically slower: no specialization, no inlining, extra ARC traffic.
3. **Profile to find, benchmark to prove.**
4. **Watch what users feel in Makima:**
   - app launch to first terminal on screen;
   - frame pacing while typing and scrolling (libghostty renders with Metal, so the Swift side must not stall the main thread);
   - workspace and agent switching latency;
   - sidebar updates under bursts of agent events;
   - memory per open terminal;
   - the cost of Swift↔Rust calls.

## Benchmarking

### Which harness

| Tool | Measures | Use it for |
|---|---|---|
| **`ordo-one/package-benchmark`** | Wall clock, CPU, **malloc counts, ARC retain/release traffic**, syscalls, context switches, threads, custom metrics | The standard Swift benchmark harness; works with SwiftPM; built-in CI comparison against `main` with thresholds |
| **Swift Testing / XCTest `measure`** | Wall clock (XCTest also has `XCTClockMetric`, `XCTMemoryMetric`, `XCTCPUMetric`) | Simple performance guards inside the test suite |
| **`hyperfine`** | Whole-command timing | Launch-to-ready timing of a CLI or headless mode |
| **XCTest `XCTApplicationLaunchMetric`** | App launch time | Launch regression tests (needs full Xcode) |

### package-benchmark setup

```swift
// Package.swift
dependencies: [
    .package(url: "https://github.com/ordo-one/package-benchmark", from: "1.11.0"),
],
targets: [
    .executableTarget(
        name: "MakimaBenchmarks",
        dependencies: [
            "MakimaCore",
            .product(name: "Benchmark", package: "package-benchmark"),
        ],
        path: "Benchmarks/MakimaBenchmarks",
        plugins: [.plugin(name: "BenchmarkPlugin", package: "package-benchmark")]
    ),
]
```

```swift
// Benchmarks/MakimaBenchmarks/MakimaBenchmarks.swift
import Benchmark
import MakimaCore

let benchmarks: @Sendable () -> Void = {
    Benchmark.defaultConfiguration = .init(metrics: [.wallClock, .mallocCountTotal, .retainCount])

    Benchmark("apply 1k agent events") { benchmark in
        let events = Fixtures.agentEvents(count: 1_000)
        benchmark.startMeasurement()
        var store = AgentStore()
        for event in events { store.apply(event) }
        blackHole(store)
    }
}
```

Commands:
```sh
swift package benchmark                                   # run all
swift package benchmark --filter "agent events"           # filter
swift package --allow-writing-to-package-directory benchmark baseline update main
swift package benchmark baseline check main               # fail on regression vs thresholds
```

Notes:
- **Install jemalloc** (`brew install jemalloc`) for the malloc metrics, or disable them with `BENCHMARK_DISABLE_JEMALLOC=1`. jemalloc must be disabled when profiling under Instruments.
- **Use `blackHole(_:)`** so the optimizer can't remove the measured work.
- **Do setup before `startMeasurement()`**, or use the setup and teardown hooks.
- **Benchmark targets that can run headless** (`MakimaCore`, stores, parsers). For UI, use Instruments.

### Good-benchmark rules

- Use realistic inputs: 20–50 agents, bursts of hundreds of events, real-sized work notes.
- Track **allocations and retain/release counts** alongside time. In Swift, hidden ARC traffic and copy-on-write copies are the usual slowdowns.
- Keep the Mac quiet and plugged in. Check results against the harness's noise thresholds before believing a change.

## Profiling

**Instruments** (comes with full Xcode, which this machine doesn't have yet) is the main tool. Profile release builds with symbols (`swift build -c release -Xswiftc -g`).

| Instruments template | Finds |
|---|---|
| **Time Profiler** | CPU hot paths across Swift **and** the Rust core in one trace |
| **Swift Concurrency** | Task lifetimes, actor contention, main-actor hogging, tasks never completing |
| **Allocations** | Allocation volume, transient objects, growth over time (use "Mark Generation" between workspace switches) |
| **Leaks** | Retain cycles, e.g. closures capturing `self` in Rust callback bridges |
| **Hangs / Animation Hitches** | Main-thread stalls that drop frames or delay keystrokes |
| **Metal System Trace** | GPU/frame pacing for libghostty's renderer |
| **os_signpost / Points of Interest** | Your own timed intervals (see below) |
| **Processor Trace** (Apple Silicon M4+) | Every branch, no sampling gaps; for deep dives |

Without full Xcode:
- `samply record .build/release/MakimaApp` gives a CPU sampling profile in the Firefox Profiler and covers Swift and Rust frames.
- Command-line tools: `sample <pid>` for a quick stack sample, `leaks <pid>` for leak checks, `heap <pid>` / `vmmap` for memory breakdowns, and `xctrace record` for Instruments traces once Xcode is installed.

### Signposts: label your own intervals

```swift
import os
let perf = OSSignposter(subsystem: "io.makima.app", category: .pointsOfInterest)

func switchWorkspace(to id: WorkID) {
    let state = perf.beginInterval("switchWorkspace", id: perf.makeSignpostID())
    defer { perf.endInterval("switchWorkspace", state) }
    …
}
```

Signposts appear in Instruments next to the CPU and concurrency tracks, so a slow workspace switch shows exactly which phase is slow. They cost almost nothing when not tracing, so you can ship them.

## Build configuration

- **Use release builds** (`-c release`). That gives `-O` plus whole-module optimization, the default in release.
- **`-Osize`** only if binary size matters more than speed. Measure it.
- **Cross-module optimization:** SwiftPM enables it by default in release for package targets. Mark small hot functions in a public API `@inlinable` only after profiling, because it freezes their implementation into clients.
- **Link the Rust staticlib built in release mode with thin LTO** (see `rust-performance.md`).
- **Keep dSYMs** for symbolicated crash reports and profiles; strip only in distribution builds.

## Common Swift performance wins (after profiling)

- **Generics vs existentials:** `some P` or generic parameters get specialized and inlined. `any P` adds boxing and dynamic dispatch, and can allocate. Keep `any` out of hot loops.
- **Use `final`** on classes, or `private`/`fileprivate` members, to allow static dispatch. Whole-module optimization infers `final` for internal classes without subclasses, but marking it is explicit and free.
- **Reduce ARC traffic:**
  - prefer structs;
  - avoid passing classes through many layers in hot loops;
  - in hot paths use `borrowing`/`consuming` parameters, and noncopyable types where ownership is clear.
- **Avoid accidental copy-on-write copies:**
  - mutate collections in place (`array[i].field = x`, `dict[key, default: []].append(x)`);
  - don't hold a second reference to a buffer while mutating it;
  - use `reserveCapacity` for known sizes.
- **Use `Span` / `InlineArray`** (Swift 6.2+) for safe, non-allocating views and fixed-size storage. `UniqueArray` (6.4) holds noncopyable elements with no CoW overhead.
- **Strings:**
  - compare and hash `Substring`/`String` without converting back and forth;
  - work on `utf8` views for byte-level parsing;
  - avoid `NSString` bridging in hot paths.
- **Main-thread discipline:**
  - move parsing, diffing and hashing off main with `@concurrent` functions;
  - coalesce bursts of agent events (`debounce`/`throttle` from swift-async-algorithms, or batch per frame) before touching UI state.
- **SwiftUI:**
  - keep `@Observable` models fine-grained so views re-render only for properties they read;
  - give list rows stable `id`s;
  - avoid heavy work in `body`;
  - use `LazyVStack`/`List` for long agent lists.
- **AppKit:** set `wantsLayer`, avoid redundant `needsLayout`/`needsDisplay`, and resize terminals with throttling (libghostty-spm already throttles surface resizes).
- **FFI:** batch calls across Swift↔Rust, and keep big data in Rust behind UniFFI `Object` handles. Copying large `Record` arrays every frame is the classic UniFFI slowdown.
- **Launch time:**
  - do the minimum in `applicationDidFinishLaunching`;
  - show the window first, then load the vault and start the Rust runtime asynchronously;
  - avoid many dynamic frameworks (static linking via SwiftPM helps).

## Performance testing in CI

1. **Compare benchmarks on PRs** with `package-benchmark`'s GitHub workflow against `main` baselines. Set thresholds for time, malloc count and retain count.
2. **Add allocation guards** for hot paths, for example "applying an event allocates ≤ N times".
3. **Track a few end-to-end numbers over time:** cold launch to first terminal, workspace switch, and replay of N events.
4. **Run the Leaks check** (`leaks --atExit -- .build/debug/MakimaApp --self-test`) in a smoke test to catch retain cycles early.

## Tooling summary

| Task | Tool |
|---|---|
| Benchmarks (time, malloc, ARC) | `ordo-one/package-benchmark` |
| Perf guards in tests | XCTest metrics, Swift Testing + timing |
| CPU profiling | Instruments Time Profiler, `samply`, `sample` |
| Concurrency issues | Instruments Swift Concurrency |
| Memory / leaks | Instruments Allocations & Leaks, `leaks`, `heap`, `vmmap` |
| UI stalls / frames | Instruments Hangs, Animation Hitches, Metal System Trace |
| Custom intervals | `OSSignposter` |
| End-to-end timing | `hyperfine` |

## Sources

- [ordo-one/package-benchmark](https://github.com/ordo-one/package-benchmark)
- [package-benchmark README](https://github.com/ordo-one/benchmark/blob/main/README.md)
- [package-benchmark 1.11.0 release](https://github.com/ordo-one/package-benchmark/releases/tag/1.11.0)
- [package-benchmark on Swift Package Registry](https://swiftpackageregistry.com/ordo-one/package-benchmark)
- [Getting started with the Benchmark package](https://augmentedcode.io/2024/04/08/getting-started-with-benchmark-package/)
- [Performance tests in Swift](https://medium.com/@tazwarutshas/performance-tests-in-swift-370a19d20dc4)
- [swift-profiling](https://github.com/RobertPieta/swift-profiling)
- [Performance testing and benchmarking (Swift patterns)](https://softwarepatternslexicon.com/swift/testing-and-design-patterns/performance-testing-and-benchmarking/)
- [Swift 6.4 released](https://www.swift.org/blog/swift-6.4-released/)
- [Swift 6.2 released](https://www.swift.org/blog/swift-6.2-released/)
- [Swift 6.2 concurrency in practice](https://blakecrosley.com/blog/swift-6-2-concurrency-in-practice)
