# Project Conventions

> Cross-cutting project conventions for Shiroha (Rust workspace).

---

## Diagrams: prefer Mermaid

- **Diagrams in task artifacts, specs, and docs MUST be written in Mermaid** (```mermaid fenced blocks) when the diagram expresses structural relationships: dependency graphs, DAGs, call/flow paths, state transitions, sequence flows.
- Mermaid renders in GitHub, VS Code, and most agent tooling, and is far easier to maintain than ASCII box-drawing.
- ASCII art is acceptable ONLY for:
  - Short inline sketches (≤ ~5 lines) where prose alone is clearer, and
  - Deployment/annotation-heavy layouts where text labels are the point (terminal/cat readability matters).
- Do NOT mix: a given diagram should be either Mermaid or ASCII, not both. Do not duplicate the same diagram in two formats.

---

## New crates: prefer CLI scaffolding

- **When adding a crate to the workspace, prefer CLI scaffolding over hand-authoring Cargo.toml + directory tree.**
- Use the project's scaffolding CLI if one exists; otherwise use cargo-native tooling:
  - `cargo new --lib crates/<name>` (or `--bin bin/<name>`) to generate the skeleton,
  - `cargo add <dep>` to register dependencies (this respects `[workspace.dependencies]` pinning),
  - then append the new member to `members = [...]` in the root `Cargo.toml`.
- This keeps manifest formatting, `edition`/`rust-version` defaults, and dependency pinning consistent with the workspace, and avoids manual drift.
- Hand-editing Cargo.toml is allowed only for steps the CLI cannot do (e.g. custom `[features]`, `[workspace.dependencies]` re-pin, build.rs wiring).
- **Gate**: if the project gains a custom scaffolder (e.g. a `just new-crate` recipe or a `sctl` subcommand), that becomes the canonical tool — update this file to name it.

---

**Language**: All documentation should be written in **English**.
