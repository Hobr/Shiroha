# Shiroha v0.1 Technical Design

## 1. Scope

This design covers the v0.1 local runtime only:

- Host-owned finite-state-machine execution;
- one WASM Component definition adapter;
- in-component WASM guards/actions/callbacks;
- canonical WIT and Rust guest SDK;
- async Rust Host APIs;
- deterministic event processing, atomic commit, limits, tracing, tests, and
  benchmarks.

Controller, Node, scheduler, `sctl`, configurable WASI capability policy, text
adapters, and dynamic plugins are future consumers of the boundaries defined
here, not placeholder v0.1 services. A minimal Wasmtime WASI profile is part of
v0.1 so ordinary Rust `wasm32-wasip2` Components can be instantiated.

## 2. Design Principles

1. **The Host owns semantics.** Guest code supplies data and functions; it does
   not run the transition loop.
2. **IR is runtime-neutral.** `shiroha-core` never exposes Wasmtime/WIT types.
3. **Adapters and executors are different concerns.** An adapter loads a
   definition; an executor invokes a referenced function kind.
4. **Committed state is explicit.** Guest memory is disposable and is never the
   task snapshot.
5. **One deterministic order.** v0.1 avoids configurable lifecycle variants.
6. **Minimal baseline authority.** The v0.1 Host satisfies standard WASI imports
   emitted by the Rust toolchain, but its default context does not explicitly
   inherit Host directories, environment, arguments, or networking.
7. **Measure the hot path.** Preparation is explicit so event dispatch never
   recompiles/revalidates an artifact.

## 3. Workspace Layout

```text
wit/
  shiroha-machine/
    world.wit                  # canonical unversioned pre-v1 WIT package
crates/
  shiroha-core/                # IR, validation, engine, traits, errors
  shiroha-adapter-wasm/        # Wasmtime loader and WASM function executor
  shiroha-guest/               # Rust guest bindings/helpers
  shiroha/                     # public facade composing core + WASM adapter
    examples/
      local-runner.rs          # Host library usage example, not a CLI product
components/
  example-machine/             # WASIp2 Component using the baseline WASI profile
spikes/
  wasip2-import-profile/       # removed or folded into fixtures after Phase 0
  empty-linker-host/           # minimal linker/import-policy validation spike
```

`components/example-machine` should be excluded from normal Host workspace
builds if target-specific generated bindings cannot compile for the Host. The
WASIp2 import-profile spike records the standard imports produced by the pinned
toolchain and proves that the minimal Host profile satisfies them.

The `shiroha` facade is the primary user dependency. Lower-level crates remain
public enough for advanced embedding but do not duplicate facade behavior.

## 4. Architecture

```mermaid
flowchart LR
    App["Rust application"] --> Facade["shiroha facade"]
    Facade --> Loader["WASM machine loader"]
    Loader --> Adapter["WASM definition adapter"]
    Loader --> Factory["WASM executor factory"]
    Adapter --> Component["Self-contained Component"]
    Factory --> Component
    Adapter --> IR["Validated Host IR"]
    IR --> Engine["shiroha-core engine"]
    Factory --> Executor["Per-instance guest executor"]
    Executor --> Engine
    Engine --> Snapshot["Host-owned snapshot and event queue"]
```

The public load operation returns a prepared artifact containing:

- immutable validated Host IR;
- an executor factory bound to the compiled Component; and
- preparation metadata used for tracing and benchmarks.

Creating a machine instance asks the factory for a disposable guest executor
and creates a Host snapshot/event queue. Multiple instances may share the same
compiled/prelinked Component but never share committed task state.

## 5. Core Domain Model

### 5.1 Identifiers

Use validated newtypes rather than raw strings in engine code:

- `MachineId`
- `StateId`
- `EventName`
- `FunctionId`
- `ActionKind`
- `InstanceId`

Conversion from WIT/text input validates non-empty values, length limits, and
the permitted character set once during loading.

### 5.2 Payloads And Inputs

```rust
pub struct PayloadEnvelope {
    pub bytes: Arc<[u8]>,
    pub content_type: String,
    pub schema_id: Option<String>,
}

pub struct Event {
    pub name: EventName,
    pub payload: Option<PayloadEnvelope>,
}

pub enum HostInput {
    Event(Event),
    Timeout { key: String, payload: Option<PayloadEnvelope> },
    Cancel { reason: Option<PayloadEnvelope> },
}
```

Logical timeout signals participate in transition matching. A cancellation
input may be handled by an explicit cancellation transition; otherwise the Host
commits only the lifecycle change to `cancelled`. Runtime wall-clock deadline
expiration is a fault and is distinct from a logical timeout input.

### 5.3 Definition IR

```rust
pub struct MachineDefinition {
    pub id: MachineId,
    pub initial: StateId,
    pub states: Vec<StateDefinition>,
}

pub struct StateDefinition {
    pub id: StateId,
    pub entry: Option<FunctionRef>,
    pub exit: Option<FunctionRef>,
    pub terminal: Option<TerminalKind>,
    pub transitions: Vec<TransitionDefinition>,
}

pub struct TransitionDefinition {
    pub trigger: Trigger,
    pub guard: Option<FunctionRef>,
    pub action: Option<FunctionRef>,
    pub target: StateId,
    pub failure_target: Option<StateId>,
}

pub struct FunctionRef {
    pub kind: ActionKind,
    pub locator: FunctionId,
}
```

`ValidatedMachine` converts the ordered input vectors into immutable indexes:

- state ID to state index;
- per-state trigger to ordered transition indexes; and
- logical function declarations by kind/locator.

Transition ordering remains the declaration order even after indexing.

### 5.4 Snapshot And Lifecycle

```rust
pub struct MachineSnapshot {
    pub instance_id: InstanceId,
    pub sequence: u64,
    pub state: StateId,
    pub context: PayloadEnvelope,
    pub lifecycle: Lifecycle,
}

pub enum Lifecycle {
    Active,
    Completed,
    Failed(FaultRecord),
    Cancelled(CancelRecord),
}
```

Quiescence is an active machine with an empty internal-event queue, not a
separate durable lifecycle state.

## 6. Adapter And Executor Contracts

### 6.1 Definition Adapter

The adapter consumes opaque artifact bytes and returns only domain data:

```rust
#[async_trait]
pub trait DefinitionAdapter: Send + Sync {
    async fn load_definition(
        &self,
        artifact: ArtifactBytes,
        limits: &LoadLimits,
    ) -> Result<MachineDefinition, AdapterError>;
}
```

The exact trait may accept a prepared adapter-specific artifact to avoid
double compilation, but its observable output remains `MachineDefinition`.

### 6.2 Function Executor

```rust
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

pub trait FunctionExecutorFactory: Send + Sync {
    fn create(&self) -> BoxFuture<'static, Result<Box<dyn FunctionExecutor>, RuntimeFault>>;
}
```

The engine dispatches by `ActionKind`. v0.1 registers only the component-scoped
WASM kind. A future registry can add HTTP, shell, remote, or plugin kinds
without changing the definition adapter or transition engine.

### 6.3 Prepared Machine

The facade coordinates the adapter and executor factory:

```rust
pub struct PreparedMachine {
    definition: Arc<ValidatedMachine>,
    executor_factory: Arc<dyn FunctionExecutorFactory>,
    metadata: PreparationMetadata,
}
```

This convenience object does not collapse the conceptual adapter/executor
boundary; it only guarantees both were derived from the same Component bytes.

## 7. WIT Contract

Use one unversioned pre-v1 package and world with no imports:

```wit
package shiroha:machine;

world machine-component {
    export definition;
    export functions;
}
```

### 7.1 Control Types

The canonical types interface defines:

- `payload { data: list<u8>, content-type: string, schema-id: option<string> }`
- events and Host signals;
- machine/state/transition/function declarations;
- guard, hook, and action inputs;
- hook effects containing optional replacement context and internal events;
- action outcomes `succeeded(effects)` and
  `failed { code, payload, effects }`; and
- typed guest/load errors.

### 7.2 Fixed Dispatcher Exports

The Component exports fixed functions instead of dynamically named WIT
exports:

```wit
interface definition {
    get-machine: func() -> result<machine-definition, guest-error>;
}

interface functions {
    evaluate-guard: func(id: string, input: guard-input)
        -> result<bool, guest-error>;
    invoke-callback: func(id: string, input: hook-input)
        -> result<hook-effects, guest-error>;
    invoke-action: func(id: string, input: hook-input)
        -> result<action-outcome, guest-error>;
}
```

The definition includes a function catalog so the Host can reject missing,
duplicate, or kind-mismatched logical IDs before startup. The Rust guest SDK
provides dispatcher helpers so component authors implement Rust functions or a
trait map instead of hand-writing string matching.

The official Rust SDK builds this world with `wasm32-wasip2`. The runtime does
not encode that Rust target in its artifact contract; the final Component type
and import set are authoritative.

## 8. Validation

Loading performs all structural checks before a machine can start:

- final artifact is a Component implementing the canonical Shiroha world;
- every Component import is satisfied by the active Host profile (v0.1 links
  supported standard WASI interfaces and rejects other imports);
- machine, state, event, and function ID validity/limits;
- unique state/function IDs;
- the initial state exists and terminal-state invariants are valid;
- all normal/failure targets exist;
- failure targets appear only on transitions with actions;
- referenced function kind/ID exists and matches guard/action/callback use;
- terminal states have no outgoing transitions;
- definition, state, transition, and payload counts stay below load limits; and
- WIT payload content type/schema fields meet length constraints.

Unreachable states are reported as validation warnings, not hard errors, in
v0.1. Warnings are returned in `PreparationMetadata` and emitted through
tracing.

## 9. Execution Algorithm

### 9.1 Startup

1. Create a disposable executor and stage the caller-provided initial context.
2. Invoke the initial state's entry callback when present.
3. On success, create and return sequence `0` as `Active` or the initial
   terminal outcome.
4. On fault, return `StartError` containing the attempted initial state/context
   and fault. No `MachineInstance` or committed snapshot is created.

### 9.2 Dispatch And Run-To-Completion

```text
queue external input
while queue not empty:
  check deadline and microstep budget
  pop FIFO input
  find ordered candidate transitions for current state + trigger
  evaluate guards until the first true result
  if none: record UnhandledEvent and continue
  clone references to committed snapshot into a staged step
  invoke exit callback
  invoke action (or synthesize success when absent)
  select normal/failure target
  invoke target entry callback
  commit state/context/lifecycle/sequence
  append staged internal events FIFO
return RunReport at quiescence/terminal/fault
```

Only small metadata is copied to begin a stage. Payload bytes use shared owned
storage on the Host; crossing the canonical ABI may still require a copy.

### 9.3 Fault Handling

Any runtime fault:

1. discards staged context and events;
2. retains the last committed state/context/sequence;
3. marks the task `Failed` or `Cancelled` as appropriate;
4. drops the guest executor after traps/resource faults so later inspection
   cannot accidentally re-enter a poisoned instance; and
5. records whether external guest side effects may have occurred.

Automatic restart/retry is absent in v0.1. Instance recreation is used only by
explicit caller-controlled recovery/tests and must start from the Host snapshot.

## 10. Wasmtime Runtime

### 10.1 Engine And Preparation

Create one reusable Wasmtime `Engine` configured with:

- Component Model support;
- Wasmtime async support used by generated typed calls;
- epoch interruption for the default CPU-budget mode; or
- fuel consumption for the explicitly selected deterministic CPU-budget mode.

`ShirohaRuntime` selects one CPU-budget mode when it builds the Engine. The
default Engine uses epoch interruption and does not enable fuel instrumentation.
The optional fuel mode builds a fuel-enabled Engine for deterministic budgeting.
v0.1 does not require both mechanisms to be active in one Engine.

`WasmMachineLoader::prepare(bytes)`:

1. compiles the Component once;
2. inspects and records the final Component imports for diagnostics and future
   capability-policy integration;
3. creates a linker with Wasmtime's standard WASI interfaces, does not register
   application-specific external interfaces, and never defines unknown imports
   as trap stubs;
4. creates an `InstancePre` to resolve imports/type matching once;
5. creates a limited temporary Store/instance;
6. calls `definition.get-machine`;
7. converts WIT values to Host IR and validates it; and
8. returns `PreparedMachine` holding the validated IR and an executor factory
   backed by the Component/`InstancePre`.

### 10.2 Per-Instance Store

Each local machine owns one Wasmtime Store and instance. Store data contains:

- the Wasmtime WASI context and resource table;
- `StoreLimits`;
- current invocation kind/function ID for diagnostics;
- deadline/fuel metadata; and
- a poison/recreate marker.

The implementation may reuse the instance between calls for the warm path.
Tests must prove that recreating it between committed steps preserves behavior.

### 10.3 Limits

Initial finite defaults, subject to calibration in the implementation spike:

| Limit | Initial default |
|---|---:|
| CPU budget mode | Epoch deadline |
| Guest wall time per call | 1 second |
| Optional deterministic fuel | 10,000,000 units |
| Linear memory per Store | 64 MiB |
| Payload envelope data | 1 MiB |
| Internal events emitted per hook | 256 |
| Run-to-completion microsteps | 1,024 |

Use `StoreLimitsBuilder` for memory/table/instance limits. The default mode uses
epoch deadlines for low-overhead interruption; deterministic mode uses
`Store::set_fuel`. A Tokio deadline controls the public future, but dropping a
timed-out future is not considered sufficient: the selected Wasmtime mechanism
must stop guest execution.

One process-level epoch ticker is owned by the WASM runtime and shuts down with
it. Do not spawn one untracked ticker per machine or invocation.

### 10.4 Error Classification

Map Wasmtime errors by typed downcast/source inspection where possible:

- fuel exhaustion;
- epoch interruption/deadline;
- memory/table/instance limit;
- guest trap;
- canonical ABI/type mismatch; and
- instantiation/link failure.

Do not classify errors by matching human-readable strings. Unknown Wasmtime
errors become a typed `RuntimeFaultKind::Engine` retaining the source chain.

## 11. Public Host API

Proposed facade shape:

```rust
let runtime = ShirohaRuntime::builder()
    .limits(RuntimeLimits::default())
    .build()?;

let prepared = runtime.prepare_component(bytes).await?;
let mut machine = prepared.start(initial_context).await?;
let report = machine.dispatch(event).await?;
let snapshot = machine.snapshot();
```

Key types:

- `ShirohaRuntime`: owns Wasmtime engine/ticker/global configuration;
- `PreparedMachine`: immutable validated definition plus executor factory;
- `MachineInstance`: Host snapshot, queue, and one guest executor;
- `RuntimeLimits`/`LoadLimits`: finite, validated configuration, including an
  epoch-default or deterministic-fuel CPU budget;
- `RunReport`: start/end snapshot metadata, transition summaries, unhandled
  inputs, terminal/fault outcome, and counters; and
- typed load/start/dispatch errors.

`MachineInstance::dispatch(&mut self, ...)` uses exclusive mutable access to
prevent concurrent dispatch without an internal mutex. Callers that need shared
ownership choose their own async mutex/actor boundary.

Run reports contain bounded summaries rather than an unbounded copy of every
payload. Detailed data remains available through tracing or an explicit bounded
observer hook added later.

## 12. Errors And Diagnostics

Library crates use `thiserror` and typed public errors. `anyhow` is restricted
to binaries/examples/tests where erasure is appropriate.

Primary error groups:

- `LoadError`
- `ValidationError` with multiple path-addressed issues
- `StartError`
- `DispatchError`
- `RuntimeFault`/`RuntimeFaultKind`
- `BusinessFailure`

Every guest call diagnostic includes machine ID, instance ID, state ID,
function ID/kind, input trigger, snapshot sequence, fuel/deadline limits, and
whether the current step committed.

## 13. Observability

Libraries emit `tracing` spans/events but do not install a subscriber.

Required spans:

- `shiroha.prepare`
- `shiroha.validate`
- `shiroha.start`
- `shiroha.dispatch`
- `shiroha.step`
- `shiroha.guest.guard`
- `shiroha.guest.action`
- `shiroha.guest.callback`

Record IDs and counters as structured fields; never record payload bytes by
default. The later Controller can attach an OpenTelemetry-compatible tracing
subscriber/exporter without changing Core instrumentation.

## 14. Build And Guest Tooling

The official Rust guest path uses the existing native Component target:

1. generate Rust guest bindings from the canonical WIT with `wit-bindgen`;
2. compile the example directly with `cargo build --target wasm32-wasip2`;
3. inspect the final Component using `wasm-tools component wit`;
4. record the standard WASI imports emitted by the pinned Rust toolchain and
   reject unexpected application-specific imports; and
5. instantiate it with the minimal v0.1 Wasmtime WASI linker/context.

The SDK may use ordinary Rust `std`. v0.1 does not promise that every WASI
operation is authorized or useful: the default Host context provides no
explicit filesystem preopens, inherited environment/arguments, or networking.
CI validates the final artifact and the Host linker together rather than
relying on a source-code convention.

Expected infrastructure changes:

- retain/pin `wasm32-wasip2` in Rust and Nix targets;
- install/pin `wasm-tools` for validation and WIT inspection;
- replace aspirational `justfile` package commands with commands matching the
  actual guest and Host crates; and
- include `wasmtime-wasi` only in the WASM adapter/runtime dependency graph.

Alternative toolchains, including `wasm32-unknown-unknown` followed by
componentization, are compatible when their final Component implements the
same world and the Host can satisfy their imports, but they are not the official
v0.1 Rust SDK build path.

## 15. Testing And Benchmarks

### Unit Tests

- ID and definition validation;
- deterministic transition ordering;
- guard/action/callback lifecycle order;
- normal/failure target routing;
- atomic commit/discard;
- FIFO internal events and microstep limit;
- unhandled events;
- terminal/cancellation behavior; and
- mock executor instance recreation.

### WASM Integration Tests

- valid example definition and full local run;
- WASIp2 example's standard toolchain imports instantiate with the baseline
  Wasmtime WASI linker/context;
- Components declaring unsupported WASI or non-WASI imports fail during
  preparation;
- missing/duplicate logical functions;
- guest-declared errors and action business failures;
- guest trap;
- fuel exhaustion/infinite loop;
- memory limit;
- oversized input/output payload;
- wall-time interruption; and
- same-release Component shape mismatch diagnostics.

### Benchmarks

- indexed Host transition selection with no guest calls;
- Host step with mock executor;
- warm Wasmtime guard/callback/action calls;
- Component compilation; and
- `InstancePre` instantiation.

Use a criterion-style harness with inputs checked into the repository. Record
the reference hardware/toolchain and set a regression threshold only after the
first stable baseline.

## 16. Compatibility And Roadmap

v0.x WIT/IR is intentionally unversioned and may break. Documentation requires
Host and Component artifacts from the same Shiroha release/revision. v1 must
introduce explicit package/IR versioning before claiming production stability.

Future Controller/Node work reuses:

- `MachineDefinition` and snapshots as the controller-owned workflow state;
- `FunctionRef.kind` to select local/plugin/remote executors;
- `ActionOutcome` for remote and aggregated results;
- async `FunctionExecutor` for node dispatch;
- tracing spans for OpenTelemetry; and
- the Host-owned snapshot rule for stateless nodes and migration.

## 17. Trade-Offs

- Full context replacement is simpler and codec-neutral but copies large
  payloads across the canonical ABI.
- One Component per machine delays reusable cross-component actions but avoids
  premature dependency resolution/plugin lifecycle design.
- Host-owned state enables recovery/migration but forbids relying on guest
  globals for workflow correctness.
- Async-first APIs anticipate remote execution but add a runtime/future boundary
  to an otherwise local v0.1 engine.
- No pre-v1 versioning maximizes iteration speed but requires same-release
  Host/Component artifacts and provides no migration promise.

## 18. Rollback Boundaries

- The WASIp2 linker proof is a hard gate. If the minimal Wasmtime WASI context
  cannot satisfy the official example without explicitly inheriting broad Host
  authority, return to design before expanding the default context.
- `shiroha-core` must pass all tests using a mock executor before Wasmtime is
  integrated. If the Wasmtime layer forces IR changes, return to design review.
- WIT is committed only after a minimal Host and guest round trip proves all
  required types lower/lift correctly with the pinned versions.
- Performance optimizations may change internal indexing/ownership but must not
  change the deterministic lifecycle contract without returning to planning.
