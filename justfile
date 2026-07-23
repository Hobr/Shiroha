default:
    @just --list

build:
    cargo build --workspace

release:
    cargo build --workspace --release

install-dev:
    cargo binstall cargo-deny cargo-nextest cargo-update cargo-llvm-cov wasmtime-cli wasm-tools -y --force
    cargo deny fetch

check:
    cargo check --workspace

fmt:
    cargo fmt --all
    pre-commit run --all-files

test:
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
