# Rust practices for Makima

Research date: 2026-10-06

How to write Makima's Rust code (the core library behind the Swift app, plus any CLI or daemon). Read alongside `rust-swift-practices.md`.

## Project setup

- Use `edition = "2024"` and pin the toolchain in `rust-toolchain.toml`.
- Use a Cargo workspace with `[workspace.dependencies]`, so every crate shares one version of each dependency.
- Split the work into small crates with one job each:
  - `makima-core`: domain types and logic, no I/O framework.
  - `makima-ffi`: the UniFFI surface.
  - `makima-cli`: binary only.
- Enforce lints at the workspace level:
  ```toml
  [workspace.lints.rust]
  unsafe_code = "deny"            # allow per-module where FFI needs it
  [workspace.lints.clippy]
  pedantic = { level = "warn", priority = -1 }
  unwrap_used = "warn"
  expect_used = "warn"
  ```
- Format everything with `rustfmt`. Run `cargo clippy --all-targets -- -D warnings` in CI.

## Structs and types

- **Model the domain with types, not strings or bools.**
  - Give IDs their own newtypes: `struct WorkId(Uuid)` and `struct AgentId(Uuid)`, so the two can never be swapped.
  - Use enums for state: `enum AgentStatus { Starting, Working, InputRequired, Failed { reason: String }, … }`, not a `status: String` plus scattered flags.
  - Make invalid states unrepresentable. Carry data inside enum variants rather than `Option` fields that are "only set when X".
- **Keep fields private by default** and expose constructors and methods. Make fields public only on plain data records such as DTOs, config and UniFFI `Record`s.
- **Validate on construction ("parse, don't validate").** `WorkTitle::new(s) -> Result<Self, _>`. Once a value exists, it is valid.
- **Builders** are for types with many optional fields. Otherwise use `new` plus `Default`.
- **Derive the standard traits eagerly:** `Debug, Clone, PartialEq, Eq, Hash`, plus `Default` where it makes sense and `serde::{Serialize, Deserialize}` for persisted data. Every public type should implement `Debug`.
- **Prefer owned data in structs.** Use `String`, not `&'a str`. Lifetimes on long-lived structs spread through every signature that touches them. Use borrowing in function parameters and short-lived views.
- **Function arguments:** take `&str` / `&[T]` / `&Path`, or `impl AsRef<Path>` / `impl Into<String>` when you'll store the value. Return owned values.
- **`#[non_exhaustive]`** goes on public enums and structs that may grow.
- **`#[must_use]`** goes on builders and on functions whose result must not be ignored.

## Traits

- **Traits describe behaviour you swap.** Don't add one for a type with a single implementation; wait until there's a second implementation or a test double.
  - Good fits in Makima: `AgentAdapter` (one per program: `ClaudeCode`, `Codex`, `Shell`, and `Configured` from `[[agents]]`; see `makima-core/src/adapter.rs`), `Clock`, `VaultStore` (filesystem vs in-memory for tests).
- **Generics or `dyn`?**
  - Use generics (`fn run<A: AgentAdapter>(a: A)`) when the type is known at compile time and speed matters.
  - Use `Box<dyn Trait>` / `Arc<dyn Trait>` when you need different implementations in one collection, or a stable FFI or plugin boundary. Makima's adapters, for example, live together in one registry.
- **Async in traits:**
  - Native `async fn` in traits has been stable since Rust 1.75, but such traits **can't be used as `dyn`**.
  - For a dyn-safe async trait, use the `async-trait` crate, or return `Pin<Box<dyn Future<Output = T> + Send + '_>>` by hand.
  - When a native async trait's futures must be `Send` for tokio, declare `fn f(&self) -> impl Future<Output = T> + Send`.
- **Implement the standard conversion traits instead of ad-hoc methods:** `From`/`TryFrom`, `Display`, `FromStr`, `AsRef`, `Default`, `IntoIterator`.
- **Extension traits** add methods to foreign types; keep them in a narrow module.
- **Sealed traits** (a private supertrait) stop outside crates implementing a trait you need to evolve.

## Errors and `context`

- **Library crates** (`makima-core`, `makima-ffi`) use `thiserror` enums that callers can match on:
  ```rust
  #[derive(Debug, thiserror::Error)]
  pub enum VaultError {
      #[error("work item {0} not found")]
      NotFound(WorkId),
      #[error("invalid frontmatter in {path}")]
      Frontmatter { path: PathBuf, #[source] source: serde_norway::Error },
      #[error(transparent)]
      Io(#[from] std::io::Error),
  }
  ```
- **Binaries** (`makima-cli`, `main`) use `anyhow::Result` and attach context at every boundary:
  ```rust
  let text = fs::read_to_string(&path)
      .with_context(|| format!("reading work note {}", path.display()))?;
  ```
  - Use `with_context` (lazy) rather than `context(format!(…))`.
  - Write context that says *what you were trying to do*, not what went wrong; the source error already says that.
- Never let `anyhow::Error` cross a library's public API, including UniFFI.
- Don't `unwrap`/`expect` on input, files, the network or user data. `expect("reason")` is fine for real invariants (for example "regex literal is valid"), and the message should say why it can't fail.
- Panics are bugs, not error handling.

## "Context" as in application state

- Avoid globals and `static mut`. Build state once at startup and pass it down explicitly.
- **Pattern:** an `AppContext` / `Core` struct holds shared handles (config, vault, registry, event sender). Wrap it in `Arc` and give each component the handles it needs, not the whole bag.
  ```rust
  pub struct Core {
      config: Config,
      vault: Arc<Vault>,
      events: tokio::sync::broadcast::Sender<Event>,
  }
  ```
- Shared mutable state lives behind `Arc<Mutex<_>>` / `Arc<RwLock<_>>`, or better, is owned by a single task that you talk to through channels (the actor pattern, which fits Makima's agent model).
- `OnceLock`/`LazyLock` (in std) are fine for truly immutable globals, such as compiled regexes.
- Use **cancellation tokens**, not ad-hoc bools, for shutdown: `tokio_util::sync::CancellationToken`, passed down to every task.

## Async: when and when not

**Use async (tokio) for I/O-bound concurrency:**
- many sockets, child processes or pipes at once;
- the ACP JSON-RPC connections to agents;
- the link to the Elixir orchestrator;
- file watching;
- timers.

Makima's core (many agents talking at once) is this case.

**Don't use async for:**
- **CPU-bound work** (hashing, parsing large files, diffing). Use plain threads or `rayon`. From async code, use `tokio::task::spawn_blocking`.
- **Simple sequential tools.** A CLI that reads a file and prints doesn't need a runtime.
- **Pure domain logic.** Keep `makima-core`'s model, parsing and state machines synchronous and runtime-agnostic, and put async at the edges. This keeps logic testable and the FFI simple.

**Rules once you're in async code:**
- **Never block a worker thread.** No `std::thread::sleep`, no blocking `std::fs` in hot paths (use `tokio::fs` or `spawn_blocking`), and no long CPU loops.
- **Don't hold a `std::sync::Mutex` guard across `.await`.** Scope the guard so it drops first. If you truly must hold a lock across awaits, use `tokio::sync::Mutex`.
- **Never call `block_on` from inside the runtime.** Bridge sync and async code with channels.
- **Prefer bounded channels** (`mpsc::channel(n)`) for backpressure. Use `broadcast` for fan-out events and `watch` for "latest value" state such as agent status.
- **Put timeouts on all external calls:** `tokio::time::timeout`.
- **Use structured concurrency.** Spawn with `JoinSet` or `TaskTracker` and always join or abort; spawned tasks shouldn't be fire-and-forget.
- **Keep tasks coarse.** Don't spawn thousands of tiny tasks; batch the work.
- **Combine futures:** use `tokio::select!` with cancellation-safe futures only, and `futures::stream` / `StreamExt` for streams.
- **For the UniFFI boundary,** Rust owns a long-lived runtime (`Builder::new_multi_thread()`, stored in the root object). It exposes sync methods and pushes results to Swift through callbacks; see `rust-swift-practices.md`.

## Recommended crates

| Need | Crate | Notes |
|---|---|---|
| CLI | `clap` (derive) | `#[derive(Parser, Subcommand)]`; add `clap_complete` for shell completions |
| Serialization | `serde` + `serde_json` | Derive on persisted types; `#[serde(rename_all = "snake_case")]`, `deny_unknown_fields` for configs |
| TOML | `toml` | Config files |
| YAML (frontmatter, `repositories.yaml`) | `serde_norway` or `serde-saphyr` | `serde_yaml` is archived/unmaintained; check maintenance before choosing |
| Async runtime | `tokio` | Enable only the features you use (`rt-multi-thread`, `macros`, `sync`, `time`, `process`, `fs`, `io-util`) |
| Async utils | `tokio-util`, `futures` | `CancellationToken`, `TaskTracker`, codecs; stream combinators |
| Errors | `thiserror` (libs), `anyhow` (bins) | `color-eyre` if you want prettier CLI reports |
| Logging | `tracing` + `tracing-subscriber` | Structured fields, spans per agent/work; `#[instrument]` on key functions |
| IDs | `uuid` with `v7` feature | Time-ordered IDs, matching the vault design |
| Time | `jiff` | Modern, timezone-correct; RFC 3339 out of the box (`chrono` if a dep requires it) |
| File watching | `notify` (+ `notify-debouncer-full`) | Vault watching with debouncing |
| PTY | `portable-pty` | Spawning agent CLIs in a pseudo-terminal (from wezterm) |
| JSON-RPC (ACP) | `agent-client-protocol` crate or hand-rolled on `serde_json` + `tokio` | Check the ACP Rust SDK's maturity first |
| HTTP client | `reqwest` | If talking to remote services |
| Concurrency | `parking_lot`, `dashmap`, `rayon`, `arc-swap` | Faster locks, concurrent map, CPU parallelism, read-mostly config |
| Paths | `camino` (`Utf8PathBuf`), `directories` | UTF-8 paths simplify FFI; platform config/data dirs |
| FFI | `uniffi` | See `rust-swift-practices.md` |
| Testing | `insta`, `proptest`, `tempfile`, `assert_cmd`, `criterion` | Snapshot, property, temp dirs, CLI, benchmarks |
| Tooling | `cargo-nextest`, `cargo-deny`, `cargo-machete`, `bacon` | Faster tests, license/advisory checks, unused deps, watch mode |

## Testing

- Put unit tests next to the code (`#[cfg(test)] mod tests`). Integration tests go in `tests/` and use only the public API.
- Make I/O replaceable through traits or plain structs passed in, such as the vault store and clock, so logic can be tested without a real filesystem or time.
- Use `#[tokio::test]` for async tests, and `tokio::time::pause()` for deterministic timers.
- Use snapshot tests (`insta`) for parsers and serializers (work note ⇄ struct round-trips).

## Quick checklist

- [ ] Newtype IDs, enums for state, private fields
- [ ] `thiserror` in libraries, `anyhow` + `.with_context()` in binaries
- [ ] No `unwrap` on external data
- [ ] Sync domain core; async only at I/O edges
- [ ] No blocking calls or held std locks across `.await`
- [ ] Bounded channels, timeouts, cancellation tokens, joined tasks
- [ ] `tracing` spans instead of `println!`
- [ ] Clippy pedantic clean, `cargo deny` passing

## Sources

- [Blessed.rs crate list](https://blessed.rs/crates)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Error handling best practices (Microsoft Rust training)](https://microsoft.github.io/RustTraining/c-cpp-book/ch09-1-error-handling-best-practices.html)
- [Rust error handling in 2026](https://rustify.rs/articles/rust-error-handling-result-option)
- [Rust best practices 2026: security, idioms & error handling](https://corgea.com/learn/rust-security-best-practices)
- [API design best practices in Rust](https://www.rustfaq.org/en/api-design-best-practices-in-rust/)
- [Rust in 2026: new features and best practices](https://dasroot.net/posts/2026/04/rust-2026-new-features-best-practices/)
- [How to use async Rust without blocking the runtime](https://oneuptime.com/blog/post/2026-01-07-rust-async-without-blocking/view)
- [Top 5 Tokio runtime mistakes](https://www.techbuddies.io/2026/03/21/top-5-tokio-runtime-mistakes-that-quietly-kill-your-async-rust/)
- [tokio::spawn vs spawn_blocking](https://rustify.rs/glossary/spawn)
- [Async runtimes vs threads in Rust](https://ratuldawar.github.io/posts/tokio-vs-thread-spawning-which-is-better.html)
- [Async Rust with Tokio: beginner guide 2026](https://rustify.rs/articles/async-rust-tokio-guide-2026)
- [10 Rust crates every developer should know in 2026](https://rustify.rs/articles/10-rust-crates-every-developer-should-know-2026)
- [State of the Crates 2025](https://ohadravid.github.io/posts/2024-12-state-of-the-crates/)
