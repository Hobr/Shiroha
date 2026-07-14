# Runtime And Component Contract

> Executable v0.1 contracts across the public facade, Core, WASM adapter, and
> guest Component boundary.

## 1. Scope / Trigger

Read this spec before changing the canonical WIT, Host IR, validation, state
machine execution, snapshots, WASM loading, invocation limits, or the public
`shiroha` facade. These are cross-crate contracts: a change is incomplete until
the guest SDK, adapter conversion, Core behavior, facade, tests, and docs agree.

v0.1 is a local library. Controller, Node, scheduler, CLI, persistence, dynamic
plugins, authorization, and configurable capability grants are outside this
contract.

## 2. Signatures

The runtime-neutral Core boundary is:

```rust
#[async_trait]
pub trait DefinitionAdapter: Send + Sync {
    async fn load_definition(
        &self,
        artifact: ArtifactBytes,
        limits: &LoadLimits,
    ) -> Result<MachineDefinition, AdapterError>;
}

#[async_trait]
pub trait FunctionExecutor: Send {
    async fn evaluate_guard(
        &mut self,
        function: &FunctionRef,
        input: GuardInput,
        limits: &InvocationLimits,
    ) -> Result<bool, RuntimeFault>;

    async fn invoke_callback(
        &mut self,
        function: &FunctionRef,
        input: HookInput,
        limits: &InvocationLimits,
    ) -> Result<HookEffects, RuntimeFault>;

    async fn invoke_action(
        &mut self,
        function: &FunctionRef,
        input: HookInput,
        limits: &InvocationLimits,
    ) -> Result<ActionOutcome, RuntimeFault>;
}
```

The supported application entry points are:

```rust
ShirohaRuntime::builder().build()
ShirohaRuntime::prepare_component(bytes).await
PreparedMachine::start(initial_context).await
PreparedMachine::restore(snapshot).await
LocalMachine::dispatch(&mut self, input).await
LocalMachine::snapshot()
```

The canonical guest exports in `wit/shiroha-machine/world.wit` are:

```wit
get-machine: func() -> result<machine-definition, guest-error>;
evaluate-guard: func(id: string, input: guard-input) -> result<bool, guest-error>;
invoke-callback: func(id: string, input: hook-input) -> result<hook-effects, guest-error>;
invoke-action: func(id: string, input: hook-input) -> result<action-outcome, guest-error>;
```

## 3. Contracts

### Ownership and execution

- The Component declares the machine and implements functions; it never owns
  or runs the state-machine loop.
- The Host owns the active state, committed context, lifecycle, sequence,
  internal FIFO queue, validation, and all scheduling decisions.
- Guards are evaluated in declaration order. A transition runs exit callback,
  action, then entry callback. A self-transition still exits and re-enters.
- State, context, and emitted events commit only after the entire microstep
  succeeds. A runtime fault discards staged changes.
- Internal events drain FIFO until quiescence, terminal lifecycle, or the
  microstep limit. Unhandled inputs are observable and non-fatal.
- An action's typed business failure follows `failure_target`; without one, the
  instance enters the failed lifecycle without committing staged effects.

### Payload and snapshot

`PayloadEnvelope` is opaque bytes plus `content_type` and optional `schema_id`.
Core does not parse application payloads. Defaults bound data to 1 MiB and each
metadata string to 4 KiB. Apply the same checks to inputs, effects, business
failures, guest faults, startup context, and restored snapshots.

`MachineSnapshot` contains `machine_id`, `instance_id`, `sequence`, `state`,
`context`, and `lifecycle`. Restore must reject a different machine ID, an
unknown state, invalid limits, or an oversized context/lifecycle payload. Guest
memory is disposable and is never part of the snapshot.

### WASM and WASI

- The official Rust guest target is `wasm32-wasip2`.
- Preparation compiles once, records imports, rejects non-baseline imports,
  binds the exact typed world, calls `get-machine`, converts to Host IR, and
  validates before returning a prepared machine.
- v0.1 accepts only the stable Preview 2 interface names registered by the
  pinned `wasmtime_wasi::p2::add_to_linker_async` implementation. For Wasmtime
  46.0.1 this is the named `wasi:cli`, `wasi:clocks`, `wasi:filesystem`,
  `wasi:io`, `wasi:random`, and `wasi:sockets` interfaces through version
  0.2.12. Do not authorize a whole family with a string-prefix check: an
  unknown interface inside a recognized family or a newer patch version is
  `WasmError::UnsupportedImports` before linker/interface loading.
- The minimal `WasiCtxBuilder::new()` context inherits no arguments,
  environment, stdio streams, directories, or network grants.
- When the pinned Wasmtime/WASI version changes, review its
  `add_to_linker_async` registrations, update the exact allowlist and maximum
  supported Preview 2 patch together, then rerun both positive example-import
  and hostile unknown-interface tests.
- Do not add per-task capability APIs until authorization and capability policy
  are implemented together.

### Finite defaults

All values must remain non-zero and finite: 16 MiB artifact, 1,024 states,
8,192 transitions, 4,096 functions, epoch budget of 100 ticks capped before a
one-second wall deadline, 64 MiB aggregate linear memory per Store, 10,000 table
elements, 16 instances/tables/memories, 256 emitted events per hook, and 1,024
run-to-completion microsteps. Fuel mode is selectable but must receive finite
units.

## 4. Validation & Error Matrix

| Condition | Required result | Committed state |
|---|---|---|
| Empty or oversized artifact | `WasmError::EmptyArtifact` / `ArtifactTooLarge` | No instance |
| Unknown WASI interface, unsupported WASI version, or non-WASI import | `WasmError::UnsupportedImports` before linker/interface loading | No instance |
| Wrong Component world | `WasmError::IncompatibleComponent` | No instance |
| Structurally invalid definition | Aggregated `ValidationErrors { issues }` | No instance |
| Guest `get-machine` error | `WasmError::GuestDefinition` | No instance |
| Initial entry callback fault | `StartError` with attempted state/context | No snapshot committed |
| Action business failure with target | Route to `failure_target`; record failure | Commit only successful failure path |
| Action business failure without target | `Lifecycle::Failed(FailureRecord::Business)` | Previous state/context retained |
| Guest-declared hook error | `RuntimeFaultKind::Guest` with structured code and bounded payload | Staged changes discarded |
| Trap or canonical ABI fault | `RuntimeFaultKind::Trap`; poison executor | Staged changes discarded |
| Fuel, epoch, memory, or other limit | Typed `RuntimeFaultKind::ResourceLimit` | Staged changes discarded |
| Oversized guest output/fault payload | Replace with `ResourceLimit(Payload)` | Oversized payload is not retained |
| Snapshot machine mismatch | `DispatchError::SnapshotMachineMismatch` | Restore rejected |
| Dispatch to terminal instance | `DispatchError::NotActive` | No mutation |
| Publicly dispatched `HostInput::Start` | `DispatchError::StartupInput` | No mutation |

Wasmtime errors are classified by typed downcast to `StoreLimitError` or
`wasmtime::Trap`. Error-message string matching is forbidden.

## 5. Good / Base / Bad Cases

- Good: a valid Component is prepared once, multiple Host instances share its
  compiled `InstancePre`, and each instance owns a Store/executor.
- Base: an event with no eligible transition produces `Unhandled`, keeps the
  snapshot unchanged, and leaves the machine active.
- Bad: a callback emits an oversized replacement context. The runtime returns a
  payload resource fault and commits none of the callback's staged effects.
- Bad: a snapshot from machine A is restored using machine B's definition. The
  restore operation fails before an executor can become authoritative.
- Bad: `wasi:cli/not-real@0.2.9` shares an allowed namespace prefix but is not a
  linker registration. Preparation returns `UnsupportedImports`; it must not
  fall through to a generic link error.

## 6. Tests Required

- WIT/toolchain: build `components/example-machine`, run `wasm-tools validate`,
  and inspect its imports/exports.
- Preparation: valid definition, unknown interface within a recognized WASI
  family, unsupported newer WASI patch, non-WASI import, incompatible shape,
  aggregated validation, and definition-call failure. Assert import-policy
  failures are `WasmError::UnsupportedImports` before typed world loading.
- Core semantics: guard order, exit/action/entry order, self-transition,
  business failure targets, FIFO events, unhandled input, cancellation,
  timeout, atomic rollback, and microstep exhaustion.
- Resource boundaries: fuel, epoch, aggregate Store memory, payload data and
  metadata, guest/business failure payloads, and lifecycle payload restore.
- Recovery: recreate the guest executor from a Host snapshot and reject
  cross-machine snapshots.
- Architecture: `cargo tree -p shiroha-core -e normal` must contain no Wasmtime
  or WIT runtime dependency.
- Public API: integration tests and the local runner must use only facade APIs.

## 7. Wrong vs Correct

### Wrong

```rust
// Guest memory silently becomes workflow state, and partial effects leak.
guest.call_action().await?;
snapshot.state = target;
snapshot.context = read_guest_memory();
```

### Correct

```rust
// Stage Host-owned values, validate all guest output, then commit together.
let mut staged_context = snapshot.context.clone();
let mut staged_events = Vec::new();
let outcome = executor.invoke_action(function, input, limits).await?;
apply_validated_outcome(&mut staged_context, &mut staged_events, outcome)?;
snapshot.context = staged_context;
pending.extend(staged_events);
```

The exact helper names may differ; atomic Host ownership and validation before
commit are the contract.

### Import Policy Wrong vs Correct

```rust
// Wrong: this admits unknown interfaces such as wasi:cli/not-real.
name.starts_with("wasi:cli/")

// Correct: split the version, then require an exact registered interface and
// a Preview 2 patch supported by the pinned linker.
BASELINE_WASI_INTERFACES.contains(&interface)
    && is_supported_wasi_preview2_version(version)
```
