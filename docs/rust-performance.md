# Rust performance: benchmarking, profiling and tuning for Makima

Research date: 2026-10-06

How to measure and improve the speed of Makima's Rust code. Read alongside `rust-practices.md` and `rust-swift-practices.md`.

## Ground rules

1. **Measure before optimizing.** Every optimization needs a benchmark or profile behind it, and a before/after number.
2. **Benchmark release builds only.** Debug builds can be 10–100× slower and say nothing useful.
3. **Profile to find, benchmark to prove.** A profiler shows *where* time goes; a benchmark shows *whether* a change helped.
4. **Optimize what users feel.** For Makima that means:
   - app launch and vault scan time;
   - agent message routing latency;
   - work note parse/serialize time;
   - event journal append and replay throughput;
   - FFI call overhead between Swift and Rust;
   - memory per agent session.

## Benchmarking

### Which harness

| Tool | Measures | Use it for | Notes |
|---|---|---|---|
| **`criterion`** | Wall-clock time, with statistics | The default choice for micro and macro benchmarks | The de facto standard; HTML reports, saved baselines, regression detection |
| **`divan`** | Wall-clock time, plus allocation counting | Quick, low-ceremony benchmarks; generic benches over many types/sizes | Simple attribute API (`#[divan::bench]`); `AllocProfiler` counts allocations |
| **`gungraun`** (formerly `iai-callgrind`) | Instruction counts via Valgrind | Stable, deterministic results in **CI** on noisy shared runners | Valgrind **does not run on macOS ARM**, so use it in Linux CI only |
| **`hyperfine`** | Whole-command timing | End-to-end CLI timing (`makima-cli scan`, launch) | Not a crate: `hyperfine 'makima scan ~/vault'` |
| **CodSpeed** (service) | CI performance tracking | Flags regressions on PRs automatically | Works with `divan` and `criterion` |

**Recommendation for Makima:**
- Use `criterion` (or `divan`) locally.
- Use `hyperfine` for end-to-end command timing.
- Add `gungraun` or CodSpeed in Linux CI once there's code worth guarding.

### Criterion setup

```toml
# crates/makima-core/Cargo.toml
[dev-dependencies]
criterion = { version = "0.7", features = ["html_reports"] }

[[bench]]
name = "vault"
harness = false
```

```rust
// crates/makima-core/benches/vault.rs
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;

fn parse_work_note(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_work_note");
    for size in [1_000, 10_000, 100_000] {
        let input = fixture_note(size);
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &input, |b, input| {
            b.iter(|| makima_core::vault::parse_note(black_box(input)));
        });
    }
    group.finish();
}

criterion_group!(benches, parse_work_note);
criterion_main!(benches);
```

Commands:
```sh
cargo bench                                  # run everything
cargo bench -- parse_work_note               # filter
cargo bench -- --save-baseline main          # save a baseline on main
cargo bench -- --baseline main               # compare a branch against it
```

### Rules for good benchmarks

- **Wrap inputs and outputs in `std::hint::black_box`** so the compiler can't optimize the work away.
- **Build inputs outside the timed closure.** For inputs consumed by value, use `iter_batched` with setup.
- **Benchmark realistic sizes.** Use real-looking work notes, journals of 10k+ events, and 20–50 concurrent agents, not toy inputs.
- **Report throughput** (`Throughput::Bytes/Elements`) so results are comparable across sizes.
- **Keep the machine quiet.** Plug in the laptop, close heavy apps, and avoid Low Power Mode on macOS, because thermal throttling skews results.
- **Benchmark only the public API from `benches/`.** Keep the internals free to change.
- **Async benchmarks:** build the tokio runtime once, outside the loop, and use criterion's `async_tokio` feature (`b.to_async(&rt).iter(...)`). Never create a runtime per iteration.
- **Watch the noise.** Criterion reports change intervals; ignore changes inside its noise threshold (about ±2% by default).

## Profiling

### CPU (where does time go?)

- **`samply`** (recommended on macOS): `cargo install samply`, then `samply record ./target/profiling/makima-cli scan ~/vault`. Results open in the Firefox Profiler UI. It has better macOS support than DTrace-based tools and needs no code signing.
- **`cargo flamegraph`:** quick SVG flamegraphs. On macOS it uses DTrace, which may need SIP and code-signing workarounds, so prefer samply there.
- **Instruments** (Xcode's profiler): the Time Profiler is best for the whole **Swift app + Rust core** together, since it sees both sides of the FFI. Instruments ships with full Xcode, which this machine doesn't have yet.
- **Profile a benchmark directly:** `cargo bench --bench vault -- --profile-time 10` with samply attached, or criterion's `pprof` integration.

Use a profiling profile so you get optimized code *with* symbols:
```toml
[profile.profiling]
inherits = "release"
debug = "line-tables-only"   # file/line info in stack traces
strip = false
```
Build with `cargo build --profile profiling`.

### Memory (who allocates?)

- **`dhat`** (dhat-rs): an in-process heap profiler. It counts allocations and bytes and records peak memory, writes `dhat-heap.json`, and you view that in the DHAT viewer. Gate it behind a feature:
  ```rust
  #[cfg(feature = "dhat-heap")]
  #[global_allocator]
  static ALLOC: dhat::Alloc = dhat::Alloc;
  ```
- **`divan`'s `AllocProfiler`** counts allocations per benchmark, which is handy for allocation-regression tests.
- **Instruments' Allocations / Leaks** templates cover the whole app, including FFI-owned objects.
- **Test that allocation counts stay flat in hot paths.** The `dhat` testing mode can assert on allocation counts.

### Async (is the runtime healthy?)

- **`tokio-console`:** a live view of tasks, poll times, wakers and resources. It finds tasks that never yield, tasks that are blocked, and leaked tasks. It needs `console-subscriber` and `RUSTFLAGS="--cfg tokio_unstable"`, so put it behind a dev feature.
- **`tracing` spans with timing** (`tracing-subscriber` + `tracing-timing`, or export to Perfetto/Chrome tracing via `tracing-chrome`) give per-agent and per-message latency breakdowns.
- **Warning signs:** long single polls (blocking calls inside async code), queues that keep growing (missing backpressure), and many `tokio::runtime::worker` frames in a flamegraph (starvation).

### Compile time (developer speed)

- `cargo build --timings` writes an HTML report of which crates are slow to build.
- `cargo llvm-lines` finds generic code that bloats compilation.
- Keep dependency features minimal (tokio features especially), and use `cargo machete` to drop unused dependencies.

## Build configuration

```toml
# Cargo.toml (workspace)
[profile.release]
opt-level = 3
lto = "thin"          # most of fat LTO's gain at a fraction of the build time
codegen-units = 1     # better optimization, slower build
debug = "line-tables-only"  # keep for crash reports / symbolication, or strip on ship
# panic: see FFI note below

[profile.dev.package."*"]
opt-level = 2         # optimize dependencies in dev; your crates stay fast to rebuild
```

- **`lto = "fat"`** is for final shipping builds when the extra build time is acceptable. Measure it; "thin" is usually close.
- **`panic = "abort"`: do NOT use it in the `makima-ffi` staticlib.** UniFFI catches Rust panics at the FFI boundary and turns them into Swift errors, which needs unwinding. With `abort`, any panic in the core kills the whole Swift app. It's acceptable in a standalone `makima-cli` binary if you want it.
- **Target CPU:** `-C target-cpu=native` is fine for local experiments, but never ship it. Apple Silicon builds already target a modern baseline.
- **Allocator:** a global allocator such as `mimalloc` can cut memory and speed up allocation-heavy workloads. Set it **only in binaries** (`makima-cli`), never in library crates. For the staticlib inside the Swift app, the app's process allocator applies, so benchmark carefully before adding one.
- **PGO / BOLT** (`cargo-pgo`) is a later-stage optimization, worth 10%+ on hot binaries. Revisit it once the workloads are stable.

## Common Rust performance wins (after profiling)

- **Allocations:**
  - Reuse buffers (`clear()` instead of a new `Vec`) and pre-size with `with_capacity`.
  - Avoid `clone()` in loops.
  - Return `&str` or `Cow<str>` instead of a new `String` when possible.
  - Use `SmallVec` / `ArrayVec` for small, bounded collections.
- **Hashing:** std's SipHash is DoS-resistant but slow. For internal maps keyed by your own IDs, use `rustc-hash` (`FxHashMap`) or `ahash`.
- **Strings:** use `compact_str` or `smol_str` for many short strings, and intern repeated ones.
- **I/O:**
  - Wrap files in `BufReader` / `BufWriter`; for the journal, append in batches and `flush` deliberately.
  - Memory-map large read-only files (`memmap2`) only after measuring.
- **Serialization:**
  - Borrow with `#[serde(borrow)]` and `&str` fields to deserialize without copying.
  - Use `serde_json::from_slice` over `from_str` on raw bytes.
  - For hot internal formats, consider `simd-json`, `postcard` or `bincode`. The vault files stay human-readable regardless.
- **Concurrency:**
  - Use `parking_lot` locks, `arc-swap` for read-mostly config, and `dashmap` for contended maps.
  - Keep lock scopes tiny.
  - Prefer message passing (one owning task) over shared locks for agent state.
- **FFI:** batch across the Swift↔Rust boundary. Pass one `Vec<Record>` instead of N calls, and keep large data in Rust as an `Object` handle rather than copying it out.
- **Data layout:** a struct of arrays for large homogeneous collections; `Box` large rarely-used enum variants so the common case stays small (clippy's `large_enum_variant` warns about this).

## Performance testing in CI

1. **Run benchmarks on every PR**, compared against `main`'s baseline. Use `gungraun` (instruction counts, Linux, deterministic) or CodSpeed, because wall-clock timing on shared CI runners is too noisy.
2. **Fail or flag on regressions** beyond a threshold, for example over 5% instructions or allocations.
3. **Track a few end-to-end numbers over time with `hyperfine --export-json`:** cold start, vault scan of N notes, journal replay of N events.
4. **Add allocation-count tests** (`dhat` testing mode or `divan`) to hot paths that must stay allocation-free.

## Tooling summary

| Task | Tool |
|---|---|
| Micro/macro benchmarks | `criterion`, `divan` |
| Deterministic CI benchmarks | `gungraun` (Linux), CodSpeed |
| End-to-end command timing | `hyperfine` |
| CPU profiling (macOS) | `samply`, Instruments |
| Flamegraphs | `cargo flamegraph`, samply |
| Heap profiling | `dhat`, Instruments Allocations |
| Async runtime inspection | `tokio-console`, `tracing-chrome` |
| Build time | `cargo build --timings`, `cargo llvm-lines` |
| PGO | `cargo-pgo` |

## Sources

- [The Rust Performance Book: Benchmarking](https://nnethercote.github.io/perf-book/benchmarking.html)
- [The Rust Performance Book: Build configuration](https://nnethercote.github.io/perf-book/build-configuration.html)
- [How to benchmark Rust code with Criterion (Bencher)](https://bencher.dev/learn/benchmarking/rust/criterion/)
- [Custom benchmarking harness in Rust (Bencher)](https://bencher.dev/learn/benchmarking/rust/custom-harness/)
- [Iai-Callgrind / Gungraun on lib.rs](https://lib.rs/crates/iai-callgrind)
- [Benchmarking and analyzing Rust performance with Criterion and Iai (LambdaClass)](https://blog.lambdaclass.com/benchmarking-and-analyzing-rust-performance-with-criterion-and-iai/)
- [Rust benchmarking with Criterion.rs (Rustfinity)](https://www.rustfinity.com/blog/rust-benchmarking-with-criterion)
- [Rust profiling crates on lib.rs](https://lib.rs/development-tools/profiling)
- [How to profile Rust applications with perf, flamegraph, and samply](https://oneuptime.com/blog/post/2026-01-07-rust-profiling-perf-flamegraph/view)
- [How to profile and optimize Rust code for performance](https://oneuptime.com/blog/post/2026-02-01-rust-profiling-optimization/view)
- [flamegraph-rs](https://github.com/flamegraph-rs/flamegraph)
- [Rust performance in 2026: benchmarking, profiling, and real wins](https://blog.rajpoot.dev/posts/rust/rust-performance-2026/)
- [Performance tuning Rust in production: 2026 guide](https://khimananda.com/blog/performance-tuning-rust-in-production)
- [Release profiles and binary size (Microsoft Rust training)](https://microsoft.github.io/RustTraining/engineering-book/ch07-release-profiles-and-binary-size.html)
- [Our mimalloc migration](https://www.framequery.com/blog/mimalloc-two-lines-measurably-less-memory)
