# Shiroha v0.1 Implementation Plan

## Objective

Implement the approved v0.1 local runtime described by `prd.md` and
`design.md`. Do not implement Controller, Node, scheduler, `sctl`, configurable
WASI capability policy, text adapters, or dynamic plugins in this task. A
minimal Wasmtime WASI profile is included only to satisfy ordinary Rust
`wasm32-wasip2` Components.

The work remains one task because the deliverable is one integrated vertical
slice: the WIT contract, guest fixture, Host IR, engine, and Wasmtime adapter
must prove each other end to end. Each phase below is nevertheless an explicit
review and rollback boundary.

## Phase 0. Prove The WASIp2 Baseline WASI Profile

This is a hard gate before committing the full SDK or runtime interface.

- [x] Add a minimal temporary WIT world containing one exported function and no
      imports.
- [x] Create isolated `spikes/wasip2-import-profile` and
      `spikes/empty-linker-host`
      packages that do not depend on the final crate architecture.
- [x] Build the minimal Rust guest directly with `wit-bindgen` and
      `wasm32-wasip2`.
- [x] Inspect the generated Component and record the standard WASI 0.2 imports
      emitted by an ordinary Rust `std` build.
- [ ] Instantiate and call it from a tiny Wasmtime 46 smoke test using the
      minimal Wasmtime WASI linker and a context with no explicit Host preopens,
      inherited environment/arguments, or networking.
- [ ] Add a negative probe Component with one unsupported import and prove that
      preparation rejects it rather than silently ignoring or stubbing it.
- [x] Record the observed Rust `std` import behavior in
      `research/guest-component-build.md`.
- [ ] Pin the proven target and inspection commands in `rust-toolchain.toml`,
      `flake.nix`, and development tooling.

Validation:

```bash
just build-example
wasm-tools validate <component-path>
wasm-tools component wit <component-path>
cargo test --manifest-path spikes/empty-linker-host/Cargo.toml
```

Rollback point: if the ordinary WASIp2 example requires explicit inheritance of
broad Host authority beyond the minimal default context, stop and return to
design review. Do not add Host preopens/network grants or unknown-import trap
stubs as an unreviewed workaround.

After the proof passes, remove the temporary spike packages or fold their
minimal code into the real fixture/adapter tests without keeping duplicate
implementations.

## Phase 1. Scaffold The Workspace

- [ ] Add `shiroha-core`, `shiroha-adapter-wasm`, `shiroha-guest`, and
      `shiroha` crates.
- [ ] Add the example Component at the location proven in Phase 0 and configure
      workspace membership/exclusion appropriately.
- [ ] Establish `default-members` so normal Host commands do not accidentally
      build the target-specific guest for the Host architecture.
- [ ] Add only direct dependencies used by each crate; avoid a facade crate that
      drags Wasmtime into `shiroha-core`.
- [ ] Enable the Wasmtime features required by the proven async Component path,
      selectable epoch/fuel CPU budgeting, and typed bindings.
- [ ] Keep `wasmtime-wasi` isolated to the WASM adapter/runtime dependency
      graph.
- [ ] Reconcile the `rust-version = 1.95.0` declaration with the Rust 1.97.0
      development toolchain and document the chosen MSRV policy.
- [ ] Replace nonexistent-package `justfile` recipes with commands that match
      the new workspace while retaining later roadmap names only when useful.

Validation:

```bash
cargo metadata --no-deps
cargo check --workspace
just --list
```

Rollback point: no crate may depend on `shiroha-adapter-wasm` from
`shiroha-core`, and normal workspace checks must not require a WASM target.

## Phase 2. Implement Host IR And Validation

- [ ] Add validated identifier newtypes and payload/event/Host-input types.
- [ ] Add state, transition, function-reference, terminal, and machine
      definition types.
- [ ] Add `ValidatedMachine` with immutable state/trigger/function indexes while
      preserving declared transition order.
- [ ] Implement aggregated, path-addressed validation errors for duplicate IDs,
      missing targets/functions, invalid initial/terminal states, invalid
      failure targets, and configured count/length limits.
- [ ] Return unreachable-state diagnostics as warnings in preparation metadata.
- [ ] Define `DefinitionAdapter`, `FunctionExecutor`, and executor-factory
      boundaries without Wasmtime/WIT types.
- [ ] Define typed snapshots, lifecycle, reports, business outcomes, runtime
      faults, and resource-limit configuration.

Focused tests:

```bash
cargo test -p shiroha-core validation
cargo test -p shiroha-core definition
cargo test -p shiroha-core adapter_contract
```

Review gate: inspect `cargo tree -p shiroha-core` and confirm there is no
Wasmtime dependency.

## Phase 3. Implement The Core Engine With A Mock Executor

- [ ] Implement atomic startup with initial entry callback.
- [ ] Implement ordered trigger lookup and guard evaluation.
- [ ] Implement the fixed exit/action/target-entry lifecycle.
- [ ] Implement normal and explicit action-failure targets.
- [ ] Implement staged full-context replacement and staged internal events.
- [ ] Implement self-transitions with exit/re-entry.
- [ ] Implement FIFO run-to-completion and bounded microsteps.
- [ ] Implement non-fatal observable `UnhandledEvent` results.
- [ ] Implement logical timeout/cancel inputs and terminal lifecycle handling.
- [ ] Implement fault rollback to the last committed snapshot.
- [ ] Make dispatch require exclusive mutable access and reject operations after
      terminal lifecycle.
- [ ] Prove guest-executor recreation between committed steps with a deterministic
      mock factory.

Focused tests:

```bash
cargo test -p shiroha-core engine
cargo test -p shiroha-core atomic_commit
cargo test -p shiroha-core run_to_completion
cargo test -p shiroha-core failure_target
cargo test -p shiroha-core recreate_executor
```

Rollback point: Wasmtime integration does not begin until every semantic
acceptance criterion passes with the mock executor.

## Phase 4. Finalize WIT And Rust Guest SDK

- [ ] Replace the Phase 0 proof WIT with the canonical `shiroha:machine` world.
- [ ] Define payload, definition, input, effect, outcome, and error types.
- [ ] Export fixed definition/guard/action/callback dispatcher functions.
- [ ] Include a function catalog that permits complete Host validation.
- [ ] Implement `shiroha-guest` helpers/builders and logical dispatcher support.
- [ ] Keep the SDK within the WASIp2 baseline WASI profile proven in Phase 0.
- [ ] Implement the representative example machine with guards, callbacks,
      normal target, failure target, internal event, terminal completion, and
      context replacement.
- [ ] Add conversion tests proving WIT values and Host IR preserve ordering and
      payload envelope metadata.

Validation:

```bash
just build-example
wasm-tools validate <example-component>
wasm-tools component wit <example-component>
cargo test -p shiroha-guest
```

Review gate: review the WIT top to bottom before proceeding. Since pre-v1 is
allowed to break, correctness and clarity matter more than compatibility, but
the interface must cover every PRD behavior without Wasmtime-specific leakage.

## Phase 5. Implement The Wasmtime Adapter And Executor

- [ ] Build the reusable Wasmtime Engine with Component Model, async calls, and
      a runtime-selected CPU budget mode: epoch by default or deterministic
      fuel when requested.
- [ ] Implement a process-owned epoch ticker with deterministic shutdown.
- [ ] Compile each artifact once and create an `InstancePre` with the baseline
      Wasmtime WASI linker.
- [ ] Load the definition through a limited temporary Store/instance.
- [ ] Inspect and record Component imports before instantiation; satisfy
      supported standard WASI imports and reject unsupported WASI or non-WASI
      imports with `UnsupportedImport`.
- [ ] Build a minimally configured WASI context and never use unknown-import
      trap stubs in v0.1.
- [ ] Convert WIT definitions to Host IR and run Core validation.
- [ ] Implement the executor factory and one limited Store/instance per active
      machine.
- [ ] Implement typed async guard/action/callback calls from generated bindings.
- [ ] Reset the selected fuel or epoch deadline for every guest call.
- [ ] Apply Store memory/table/instance limits and Host payload/event limits.
- [ ] Ensure public Tokio deadlines trigger Wasmtime interruption rather than
      merely dropping a still-running guest future.
- [ ] Classify Wasmtime failures using typed errors/source chains, never string
      matching.
- [ ] Poison/drop executors after traps and resource faults.
- [ ] Emit structured tracing spans without payload contents.

Focused tests:

```bash
cargo test -p shiroha-adapter-wasm definition_load
cargo test -p shiroha-adapter-wasm guest_calls
cargo test -p shiroha-adapter-wasm fuel_limit
cargo test -p shiroha-adapter-wasm epoch_deadline
cargo test -p shiroha-adapter-wasm memory_limit
cargo test -p shiroha-adapter-wasm error_classification
```

Rollback point: if Wasmtime's pinned API cannot implement a required limit or
typed call contract, update research and return to design rather than leaking a
Wasmtime workaround into `shiroha-core`.

## Phase 6. Compose The Public Facade And End-To-End Flow

- [ ] Implement `ShirohaRuntime` builder and runtime ownership.
- [ ] Implement `prepare_component`, `PreparedMachine::start`, snapshot access,
      and async `MachineInstance::dispatch`.
- [ ] Expose finite validated defaults for load/runtime limits.
- [ ] Keep lower-level escape hatches explicit and typed.
- [ ] Add `crates/shiroha/examples/local-runner.rs` using the public facade only.
- [ ] Run the example Component through startup, normal transition, failure
      target, internal event, unhandled event, and terminal completion.
- [ ] Add an end-to-end test proving instance recreation does not change the
      committed result.
- [ ] Ensure same-release shape mismatches fail during preparation with a clear
      error.

Validation:

```bash
just build-example
cargo test -p shiroha --test local_component
cargo run -p shiroha --example local-runner
```

Review gate: the example must not depend directly on Wasmtime, generated WIT
Host modules, or `shiroha-core` internals.

## Phase 7. Performance, Observability, And Documentation

- [ ] Add Host-only transition and mock-executor benchmarks.
- [ ] Add warm guard/action/callback Wasmtime benchmarks.
- [ ] Add separate compilation and `InstancePre` instantiation benchmarks.
- [ ] Record the fixed reference environment and first baseline.
- [ ] Set and document a regression threshold after the baseline is stable.
- [ ] Verify required tracing spans/fields and that payload bytes are absent by
      default.
- [ ] Update README with v0.1 scope, architecture, library example, guest build,
      limits, baseline WASI profile, and pre-v1 compatibility statement.
- [ ] Document the pre-v1 roadmap without presenting deferred Controller/Node
      APIs as implemented.
- [ ] Replace placeholder backend Trellis specs with conventions actually
      established by the implementation, using `trellis-update-spec` at the
      finish phase.

Validation:

```bash
cargo bench --workspace --no-run
cargo doc --workspace --no-deps
cargo run -p shiroha --example local-runner
```

## Phase 8. Full Quality Gate

Run the repository's final checks from a clean process environment:

```bash
just fmt
just check
just build-example
just test
just coverage
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo deny check
cargo doc --workspace --no-deps
```

Also verify the declared MSRV when available:

```bash
cargo +1.95.0 check --workspace
```

Final manual review:

- [ ] Every PRD acceptance criterion has a named automated test, benchmark, or
      documented release check.
- [ ] The v0.1 example's standard WASI imports are recorded and satisfied by a
      minimally configured context.
- [ ] Components with unsupported WASI or non-WASI imports fail during
      preparation.
- [ ] `shiroha-core` remains runtime-neutral.
- [ ] No deferred Controller/Node/plugin API was added without an executable
      v0.1 consumer.
- [ ] No runtime limit defaults to unlimited.
- [ ] No Wasmtime error classification uses string matching.
- [ ] Public examples use only supported facade APIs.
- [ ] Git diff contains no generated cache, target output, or unrelated changes.

## Risky Files And Rollback Points

| Area | Likely files | Rollback trigger |
|---|---|---|
| Guest toolchain | `rust-toolchain.toml`, `flake.nix`, `justfile`, Component manifest | WASIp2 baseline linker proof fails |
| Canonical ABI | `wit/shiroha-machine/world.wit`, guest bindings | Required types do not lower/lift or Host/guest diverge |
| Core semantics | `shiroha-core` engine/IR | Mock tests cannot prove deterministic atomic behavior |
| Wasmtime limits | adapter runtime/store code | Guest cannot be interrupted/bounded reliably |
| Public API | `shiroha` facade/examples | Example requires internal or Wasmtime-specific access |
| Performance | indexes, ownership, instance reuse | Optimization changes observable lifecycle semantics |

## Planning Review Gate

Do not run `task.py start` until the user has reviewed `prd.md`, `design.md`, and
`implement.md` and explicitly approves implementation.
