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

build: build-sctl build-shirohad
release: release-sctl release-shirohad

install-dev:
    cargo binstall cargo-deny cargo-nextest cargo-update cargo-llvm-cov -y --force
    cargo deny fetch

check:
    cargo check --workspace

fmt:
    cargo fmt
    pre-commit run --all-files

test:
    cargo nextest run --all-features --run-ignored all

coverage:
    cargo llvm-cov nextest --workspace --html

doc:
    cargo doc --open --workspace

update:
    nix flake update
    cargo install-update -a
    cargo update
    pre-commit autoupdate
    cargo deny fetch
