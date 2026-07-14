# Quality Guidelines

> Required toolchain, gates, tests, and review invariants.

## Overview

The workspace requires Rust 1.97.0, edition 2024, formatted code, warning-free
Clippy, passing Nextest/coverage runs, dependency policy checks, valid Rust docs,
and a valid `wasm32-wasip2` example Component.

## Forbidden Patterns

- Unlimited or zero resource-limit defaults.
- Wasmtime/WIT types or dependencies in `shiroha-core`.
- Wasmtime error classification by string matching.
- Payload/context bytes in tracing fields.
- Guest memory as authoritative state.
- Partial Host commits before a full step succeeds.
- Automatic action retries or compensation claims.
- Deferred Controller/Node/plugin/CLI APIs without an executable consumer.
- Generated `target`, coverage, Criterion, or CodeGraph outputs in commits.
- `unwrap`/`expect` on external input paths; internal validated invariants must
  include an explanatory message when `expect` is appropriate.

## Required Patterns

- Validate finite limits at public construction/dispatch boundaries.
- Convert source-specific data at adapter edges and expose owned Core types.
- Preserve transition declaration order after indexing.
- Stage context/events and commit atomically.
- Bound all guest-controlled payload locations, including error paths.
- Bind snapshots to `machine_id` and validate state/payloads on restore.
- Reset fuel/epoch budget before every guest call, including definition loading
  after instantiation; enforce Tokio wall time as an independent boundary.
- Aggregate Store memory growth across all memories.
- Use typed traps/limiter errors for resource classification.

## Commit Requirements

### Convention: Module-Scoped Commits

**What**:

- Create a separate Git commit for each functional module. Do not combine
  unrelated modules in one commit.
- Use the commit-message format
  `<functional-module>: <concise change summary>`.
- Before every commit, run `just fmt` from the repository root. Fix every
  reported error and warning, then rerun the command until it succeeds without
  errors or warnings. Only then stage and commit the module.

**Why**: Module-scoped commits remain easy to review and revert, while the
mandatory formatting gate prevents known formatting or repository-hook issues
from entering history.

**Examples**:

```text
core: validate restored snapshots
wasm-adapter: reject unsupported imports
docs: clarify the guest build workflow
```

**Wrong vs Correct**:

```text
# Wrong: unrelated modules are bundled together.
misc: update core, adapter, and docs

# Correct: each functional module has its own commit.
core: validate restored snapshots
wasm-adapter: reject unsupported imports
docs: clarify the guest build workflow
```

## Testing Requirements

Run from the repository root:

```bash
just fmt
just check
just validate-example
just test
just coverage
just clippy
cargo deny check
cargo bench --workspace --no-run
cargo doc --workspace --no-deps
just run-example
```

`just clippy` intentionally checks each crate separately with all targets and
features; this preserves coverage while avoiding the workspace target-planning
stall observed with Cargo 1.97 in this repository.

Core behavior requires mock-executor unit tests. WASM boundaries require real
Component tests, including hostile WAT/Component fixtures for import and shape
failures. Public facade behavior requires integration tests. Performance-sensitive
paths require Criterion benchmarks and a recorded reference environment.

## Code Review Checklist

- Does every PRD acceptance criterion map to a named test, benchmark, or release
  check?
- Do new guest outputs and snapshot fields have explicit bounds and restore
  validation?
- Can any fault path leak staged state/events or retain oversized guest data?
- Does Core remain runtime/format/transport neutral?
- Are standard WASI imports intentional and satisfied by the minimal context?
- Are unsupported imports rejected before interface loading?
- Are error variants preserved through the facade?
- Does `git diff --check` pass, and is the diff free of generated/unrelated
  files?

See [Runtime and Component Contract](./runtime-contract.md) for cross-layer
signatures, cases, and assertion points.
