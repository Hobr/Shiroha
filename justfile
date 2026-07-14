example-manifest := "components/example-machine/Cargo.toml"
example-target-dir := "target/components"
example-component := example-target-dir + "/wasm32-wasip2/debug/example_machine.wasm"

default:
    @just --list

build-example:
    cargo build --manifest-path {{example-manifest}} --target wasm32-wasip2 --target-dir {{example-target-dir}}

release-example:
    cargo build --manifest-path {{example-manifest}} --target wasm32-wasip2 --target-dir {{example-target-dir}} --release

validate-example: build-example
    wasm-tools validate {{example-component}}
    wasm-tools component wit {{example-component}}

run-example: build-example
    cargo run -p shiroha --example local-runner -- {{example-component}}

build: build-example
    cargo build --workspace

release: release-example
    cargo build --workspace --release

install-dev:
    cargo binstall cargo-deny cargo-nextest cargo-update cargo-llvm-cov wasmtime-cli@46.0.1 wasm-tools@1.253.0 -y --force
    cargo deny fetch

check:
    cargo check --workspace

# Cargo 1.97 can stall while planning all workspace targets in one clippy
# invocation. Keep the same coverage while isolating package target graphs.
clippy:
    cargo clippy -p shiroha-core --all-targets --all-features -- -D warnings
    cargo clippy -p shiroha-adapter-wasm --all-targets --all-features -- -D warnings
    cargo clippy -p shiroha-guest --all-targets --all-features -- -D warnings
    cargo clippy -p shiroha --all-targets --all-features -- -D warnings

fmt:
    cargo fmt --all
    cargo fmt --manifest-path {{example-manifest}}
    pre-commit run --all-files

test: build-example
    cargo nextest run --workspace --all-features

coverage:
    cargo llvm-cov nextest --workspace --html

doc:
    cargo doc --workspace --no-deps

update:
    nix flake update
    cargo install-update -a
    cargo update
    pre-commit autoupdate
    cargo deny fetch
