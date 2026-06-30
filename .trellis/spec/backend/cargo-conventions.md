# Cargo Conventions

> Cargo workspace, package init, and dependency management conventions.

---

## Package Initialization

When creating a new cargo package (crate or binary) inside the workspace, use
`cargo init` to scaffold it, then wire it into the workspace.

```bash
# Library crate
cargo init --lib crates/<name> --name shiroha-<name>

# Binary crate
cargo init --bin bin/<name> --name <name>
```

After `cargo init`:

1. Verify the package appears in the root `Cargo.toml` `[workspace] members`.
2. Add the package to `[workspace.dependencies]` if it is shared.
3. Run `just check` to confirm the workspace still compiles.

Do not hand-write `Cargo.toml` + `lib.rs`/`main.rs` from scratch when `cargo
init` can scaffold the standard layout.

## Dependency Versioning

When introducing a new dependency, use the **latest stable release**.

```bash
# Check latest version
cargo search <crate>

# Add at latest version
cargo add <crate>

# For workspace-level shared deps, add to [workspace.dependencies]
```

Rules:

- Prefer the newest semver-stable release (avoid pre-release/alpha unless there
  is a documented reason).
- If a dependency is already in `[workspace.dependencies]`, reference it via
  `dep:` from the member crate — do not pin a different version per crate.
- Run `cargo update <crate>` after adding to pick up transitive updates.
- Run `cargo deny check` after adding to verify license + advisory status.

## Workspace Structure

```
Cargo.toml                  # [workspace] members + [workspace.dependencies]
crates/                     # library crates (shiroha-*)
bin/                        # binary crates (shirohad, sctl)
```

Shared dependencies live in `[workspace.dependencies]` at the root. Member
crates reference them with `dep:` and optional feature lists.
