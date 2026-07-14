# Error Handling

> Typed failure boundaries and atomic rollback conventions.

## Overview

Errors are typed by boundary and phase. Preparation errors, validation issues,
startup failure, dispatch misuse, business failure, and runtime faults are not
interchangeable. Preserve this distinction so callers can decide whether a
machine never started, retained its last snapshot, followed a failure route, or
became terminal.

## Error Types

| Boundary | Type | Meaning |
|---|---|---|
| Definition structure | `ValidationErrors { issues }` | Aggregated, path-addressed invalid IR |
| Component preparation | `WasmError` | Artifact, imports, linking, shape, definition call/conversion |
| Initial entry | `StartError` | No initial snapshot was committed |
| Public dispatch/restore | `DispatchError` | Invalid lifecycle/input/limits/snapshot |
| Guest invocation | `RuntimeFault` | Guest error, trap, resource limit, engine, or Host fault |
| Action domain result | `BusinessFailureRecord` | Expected business failure, not a trap |

Facade error enums wrap these sources with `#[from]`; they do not flatten them
into strings.

## Propagation And Atomicity

```rust
let result = executor.invoke_action(function, input, limits).await;
match result {
    Ok(outcome) => validate_and_stage(outcome)?,
    Err(fault) => fail_from_last_committed_snapshot(fault),
}
```

- Use `thiserror` source conversion at crate boundaries.
- Aggregate independent definition issues instead of returning the first one.
- Attach structured guest `code` and bounded payload where available.
- Set `external_effects_possible` after invoking actions/callbacks; rollback
  covers Host state only and never claims external compensation.
- Poison a WASM executor after traps, engine faults, or resource exhaustion.
- Classify Wasmtime failure through typed errors/traps, never diagnostic text.
- Replace oversized guest/business fault payloads with a typed payload-limit
  fault so failed snapshots remain bounded.

## Public API Status

v0.1 exposes Rust error types and has no HTTP/RPC error envelope. A future
Controller API must define stable transport codes without changing the Core
taxonomy or exposing raw Wasmtime diagnostics as protocol contracts.

## Common Mistakes

- Treating business failure as `RuntimeFault` loses `failure_target` routing.
- Committing exit/action effects before entry succeeds breaks atomicity.
- Matching `error.to_string()` to identify fuel/deadline/memory is brittle.
- Retrying an action automatically can duplicate external effects.
- Restoring by state ID alone can apply a snapshot to the wrong machine.

See [Runtime and Component Contract](./runtime-contract.md) for the complete
validation/error matrix and required tests.
