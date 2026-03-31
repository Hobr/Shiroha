# Repository Guidelines

## Project Structure & Module Organization

This repository is a Rust workspace. Executables live under `app/`:

- `app/shirohad`: daemon entrypoint
- `app/sctl`: CLI entrypoint

Reusable libraries live under `crate/` and should follow the existing split by responsibility, e.g. `shiroha-core`, `shiroha-engine`, `shiroha-runtime`, `shiroha-store-sqlite`, `shiroha-sdk`, and `shiroha-testkit`. Design and interface notes belong in `docs/`. Keep new code in the smallest crate that matches a stable boundary; avoid catch-all crates such as `common` or `utils`.

## Build, Test, and Development Commands

- `cargo build --workspace`: build the full workspace
- `cargo check --all`: fast compile check used by pre-commit
- `cargo clippy --all-targets --all-features --tests --benches -- -D warnings`: lint with warnings denied
- `cargo nextest run --all-features`: run tests
- `just shirohad`: run the daemon
- `just sctl`: run the CLI
- `just build-shirohad` / `just build-sctl`: build individual apps
- `just fmt`: run `cargo fmt` and all pre-commit hooks
- `just install-dev`: install local developer tooling such as `cargo-deny`, `cargo-nextest`, and `wasmtime-cli`

## Coding Style & Naming Conventions

Rust 1.94.1 is pinned in `rust-toolchain.toml`; the workspace also targets `wasm32-wasip2`. Use `rustfmt` defaults plus repository rules from `.editorconfig`: 4-space indentation for code, 2 spaces for Markdown/Nix, LF endings, and a final newline. Prefer:

- `snake_case` for modules, files, and functions
- `PascalCase` for types and traits
- `kebab-case` for crate names

## Testing Guidelines

Place unit tests near the code they cover and add integration tests under each crate’s `tests/` directory when needed. Test execution should at minimum pass through `cargo nextest run --all-features`. For core execution logic, add replay/recovery-focused tests before adding new distributed behavior.

## Commit & Pull Request Guidelines

Recent history uses short scoped commits such as `project: 重构项目`, `rust: 升级到1.94.1`, and `app: 结构修改`. Follow that style: `<scope>: <imperative summary>`. Keep PRs focused and include:

- what changed
- which crates/apps were touched
- commands run (`cargo check`, `cargo clippy`, `cargo nextest run`)
- linked issue or design note when applicable

If a PR changes architecture or interfaces, update the relevant file in `docs/` in the same change.
