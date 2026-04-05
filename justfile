run_cmd := "cargo run"
build_cmd := "cargo build"

package-sctl := "-p sctl"
package-shirohad := "-p shirohad"

target-plugin := "--target wasm32-wasip2"

default:
    @just --list

sctl *params:
    {{run_cmd}} {{package-sctl}} -- {{params}}

build-sctl:
    {{build_cmd}} {{package-sctl}}

release-sctl:
    {{build_cmd}} {{package-sctl}} --release

shirohad *params:
    {{run_cmd}} {{package-shirohad}} -- {{params}}

build-shirohad:
    {{build_cmd}} {{package-shirohad}}

release-shirohad:
    {{build_cmd}} {{package-shirohad}} --release

build-example-simple:
    {{build_cmd}} {{target-plugin}} --manifest-path example/simple/Cargo.toml --release

build-example-advanced:
    {{build_cmd}} {{target-plugin}} --manifest-path example/advanced/Cargo.toml --release

build-example-warning-deadlock:
    {{build_cmd}} {{target-plugin}} --manifest-path example/warning-deadlock/Cargo.toml --release

build-example-sub:
    {{build_cmd}} {{target-plugin}} --manifest-path example/sub/child/Cargo.toml --release
    {{build_cmd}} {{target-plugin}} --manifest-path example/sub/parent/Cargo.toml --release

build-example: build-example-simple build-example-advanced build-example-warning-deadlock build-example-sub

build: build-sctl build-shirohad build-example
release: release-sctl release-shirohad build-example

check:
    cargo check --workspace

install-dev:
    cargo binstall cargo-deny cargo-nextest cargo-update wasmtime-cli -y --force
    cargo deny fetch

fmt:
    cargo fmt
    pre-commit run --all-files

test:
    cargo test --workspace
    cargo test -p sctl --test cli_roundtrip -- --ignored

doc:
    cargo doc --open --workspace

update:
    nix flake update
    cargo install-update -a
    cargo update
    pre-commit autoupdate
    cargo deny fetch
