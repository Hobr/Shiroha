run_cmd := "cargo run"
build_cmd := "cargo build"

package-stctl := "-p sctl"
package-shirohad := "-p shirohad"

target-plugin := "--target wasm32-wasip2"

[working-directory: "app/sctl"]
sctl:
    {{run_cmd}} {{package-stctl}}

[working-directory: "app/sctl"]
build-sctl:
    {{build_cmd}} {{package-stctl}}

[working-directory: "app/sctl"]
release-sctl:
    {{build_cmd}} {{package-stctl}} --release

[working-directory: "app/shirohad"]
shirohad:
    {{run_cmd}} {{package-shirohad}}

[working-directory: "app/shirohad"]
build-shirohad:
    {{build_cmd}} {{package-shirohad}}

[working-directory: "app/shirohad"]
release-shirohad:
    {{build_cmd}} {{package-shirohad}} --release

build: build-sctl build-shirohad
release: release-sctl release-shirohad

install-dev:
    cargo binstall cargo-deny cargo-nextest cargo-update wasmtime-cli -y --force
    cargo deny fetch

fmt:
    cargo fmt
    pre-commit run --all-files

doc:
    cargo doc --open --workspace

update:
    nix flake update
    cargo install-update -a
    cargo update
    pre-commit autoupdate
    cargo deny fetch
