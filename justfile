# Makima build recipes. Run `just` to build and launch the app with
# example.toml; `just run-mine` uses your own ~/.config/makima/config.toml.

set shell := ["bash", "-euo", "pipefail", "-c"]

app := "apps/macos"
rust_lib := "target/release/libmakima_ffi.a"
generated_swift := app / "Sources/MakimaCoreBindings/Generated"
ffi_module := app / "Sources/MakimaCoreFFI"
# Hand-written Swift only; generated bindings are excluded from lint and format.
swift_sources := app / "Sources/MakimaApp " + app / "Sources/MakimaUI " + app / "Sources/MakimaTerminal " + app / "Sources/MakimaVoice " + app / "Sources/MakimaCore " + app / "Sources/MakimaCoreBindings/IDs.swift " + app / "Tests " + app / "Package.swift"

# With only the Command Line Tools installed, SwiftPM can't find Swift
# Testing on its own; point it at the copy the CLT ships. Empty with Xcode.
clt := "/Library/Developer/CommandLineTools/Library/Developer"
swift_test_flags := if `xcode-select -p` == "/Library/Developer/CommandLineTools" { "-Xswiftc -F -Xswiftc " + clt + "/Frameworks -Xlinker -F -Xlinker " + clt + "/Frameworks -Xlinker -rpath -Xlinker " + clt + "/Frameworks -Xlinker -rpath -Xlinker " + clt + "/usr/lib" } else { "" }

example_config := "example.toml"
# Development runs keep their data (databases, briefs, recordings) here,
# not in ~/.local/share/makima; `just clean-dev` removes it.
dev_home := justfile_directory() / ".dev"

default: run

# Build the Rust core as a static library.
core:
    cargo build --release -p makima-ffi

# Regenerate the Swift bindings, header and modulemap from the built core.
bindings: core
    cargo run --quiet -p uniffi-bindgen -- --swift-sources {{rust_lib}} {{generated_swift}}
    cargo run --quiet -p uniffi-bindgen -- --headers --modulemap --module-name MakimaCoreFFI --modulemap-filename module.modulemap {{rust_lib}} {{ffi_module}}

# Build the Rust core and the macOS app.
build: bindings
    swift build --package-path {{app}}

# Build and launch the app with a config file (example.toml by default),
# keeping its data in .dev/.
run config=example_config: bindings
    MAKIMA_HOME={{dev_home}} swift run --package-path {{app}} MakimaApp --config "$(realpath {{config}})"

# Remove everything development runs saved: work databases, briefs,
# recordings and the index (.dev/).
clean-dev:
    rm -rf {{dev_home}}

# Build and launch the app with your own config and real data.
run-mine: bindings
    swift run --package-path {{app}} MakimaApp

# Run Rust and Swift tests.
test: bindings
    cargo test --workspace
    swift test --package-path {{app}} {{swift_test_flags}}

# Check formatting and lints without changing files; warnings are errors.
lint: bindings
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    swift format lint --strict --recursive {{swift_sources}}
    MAKIMA_STRICT=1 swift build --package-path {{app}}

# Format Rust and Swift sources.
fmt:
    cargo fmt --all
    swift format --in-place --recursive {{swift_sources}}

# Run the Rust benchmarks.
# Benchmarks: Rust (criterion) and the Swift hot paths (release build).
bench: bindings
    cargo bench -p makima-core -p makima-tui -p makima-store
    MAKIMA_BENCH=1 swift test --package-path {{app}} -c release -Xswiftc -enable-testing {{swift_test_flags}} --filter PerformanceTests

clean:
    cargo clean
    swift package --package-path {{app}} clean
