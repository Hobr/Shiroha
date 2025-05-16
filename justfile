run_cmd := "cargo run"
build_cmd := "cargo build"

wasm_target := "--target wasm32-wasip2"

fmt:
    cargo fmt
    pre-commit run --all-files

install-dev:
    cargo binstall cargo-deny cargo-nextest -y --force
    cargo deny fetch

update:
    nix flake update
    cargo update
    pre-commit autoupdate
    cargo deny fetch

doc:
    cargo doc --open --workspace
