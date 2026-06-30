# Git Commit Conventions

> How commits are structured in this project.

---

## Commit Cadence

Commit once per completed feature module. A "feature module" = one cohesive unit
of work (e.g. an IR schema, an HSM runtime subsystem, one adapter, one plugin).

Do not batch unrelated work into a single commit. Do not let a feature module
span multiple uncommitted days without checkpointing sub-modules.

## Commit Message Format

```
<module>: <brief description>
```

- `<module>` = the crate or subsystem name (`ir`, `engine`, `wasm`, `plugin`,
  `control`, `shirohad`, `sctl`, `wit`, etc.) or a topical scope (`lint`,
  `version`, `deps`, `flake`).
- Lowercase module name, colon, space, then a short imperative summary.
- Keep the summary to one line; put detail in the body if needed.

### Examples

```
ir: add StateMachineDef schema and validation
engine: implement HSM run-to-completion event loop
wasm: add wasmtime Component Model adapter
plugin: add http action func
shirohad: add cargo feature matrix (full/controller/node)
lint: format
```

## Pre-Commit Gate (Mandatory)

Before every commit, run:

```bash
just fmt
just test
```

- `just fmt` runs `cargo fmt` + pre-commit hooks.
- `just test` runs `cargo nextest run --all-features --run-ignore all`.

Do not commit if either step fails. Fix the issue, re-run both, then commit.

## Forbidden

- `--no-verify` to skip hooks.
- Committing with failing tests.
- Squashing multiple feature modules into one commit.
- Vague messages (`fix`, `update`, `wip`) without module scope.
