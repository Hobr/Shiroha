# Shiroha v0.1 Core Runtime

## Goal

Deliver the first usable Shiroha release as a Rust library for executing
WASM-defined finite-state machines locally. A WASM Component supplies the
machine definition and guest guard/action/callback functions, while the Host
owns the state-machine loop, state, context, lifecycle, validation, limits, and
observability.

The v0.1 design must prove the core Host/WASM boundary without prematurely
implementing the distributed controller platform. It must leave deliberate
extension points for text adapters, action plugins, aggregation, remote
dispatch, WASI capabilities, authorization, and the controller/node roadmap.

## Background And Release Boundary

- The repository currently contains an empty Rust workspace and infrastructure
  configuration but no Shiroha crates or runtime code.
- The project starts at `0.1.0`; `1.0.0` is the production-ready target.
- v0.1 is core-first: local Host execution, the WASM Component adapter, the
  canonical WIT contract, a Rust guest SDK, tests, benchmarks, and examples.
- The official v0.1 Rust guest target is `wasm32-wasip2`, but the Host contract
  is target-neutral: it accepts any final Component implementing the canonical
  Shiroha world and satisfying the active import policy.
- Distributed scheduling, Controller/Node services, role-specific Cargo
  features, and `sctl` remain required pre-v1 milestones but are not v0.1
  deliverables.
- Multi-controller operation is deferred until after v1.0.
- Host and Component artifacts from different pre-v1 Shiroha releases are not
  required to be compatible. Stable ABI/IR versioning begins with the v1
  production contract.

## Requirements

### R1. Deterministic State-Machine Semantics

v0.1 must implement a flat, event-driven finite-state machine with exactly one
active state at a time.

The definition model must support:

- named states and named events;
- one initial state;
- completed, failed, and cancelled terminal outcomes;
- transitions evaluated in declared order;
- one optional guard per transition;
- one optional transition action;
- optional state entry and exit callbacks;
- one normal target and one optional action-failure target per transition; and
- Host-produced timeout and cancellation inputs.

For the same definition, committed context, and ordered inputs, the Host must
select transitions and invoke guest functions in the same documented order.
Hierarchical states, parallel regions, history states, eventless transitions,
and internal-transition mode are not part of v0.1.

### R2. Lifecycle And Event Processing

Machine startup is an atomic step: invoke the initial state's entry callback,
then commit the initial snapshot only if it succeeds.

A normal or self-transition uses this fixed order:

1. evaluate candidate guards in declared transition order;
2. invoke the source state's exit callback;
3. invoke the selected transition action;
4. choose the normal or explicit failure target from the action outcome;
5. invoke the selected target state's entry callback; and
6. atomically commit the new state, context, and emitted internal events.

A self-transition exits and re-enters the same state. v0.1 does not provide an
internal-transition variant that skips exit/entry callbacks.

External events are processed one at a time with run-to-completion semantics.
After the main transition commits, emitted internal events are drained FIFO
until the machine is quiescent, terminal, failed, cancelled, or exceeds its
configured microstep limit. Events emitted by a failed step are discarded.

An event with no eligible transition is consumed without changing state or
context. The result must contain a structured `UnhandledEvent` outcome and a
tracing diagnostic. It is non-fatal and is not retried automatically.

### R3. Host-Owned State And Atomic Commit

The Host is the sole authority for:

- the current state;
- committed application context;
- pending internal events;
- task lifecycle and last committed snapshot; and
- resource-limit and execution metadata.

Guest linear memory and globals are not authoritative task state. The runtime
may reuse a warm guest instance for performance or recreate it for isolation or
recovery; correctness must not depend on instance reuse.

Guards are read-only. Each action/callback receives the latest staged context
and triggering input. It returns an optional complete replacement context,
zero or more internal events, and its typed outcome. `none` means no context
change. Later hooks in the same step observe the latest staged replacement.

State, context, and emitted events become visible only after the entire step
succeeds. Runtime faults discard all staged changes and retain the previous
committed snapshot.

### R4. Outcomes And Error Semantics

The runtime must distinguish:

- action business failure: a typed `failed { code, payload }` result;
- guest-declared hook error;
- guest trap or canonical ABI violation;
- Host/internal error;
- timeout or cancellation; and
- resource-limit exhaustion.

An action business failure selects the transition's explicit failure target.
If no failure target exists, the task enters `failed` and the step does not
commit. Guard, exit-callback, entry-callback, ABI, and trap errors are runtime
faults rather than business-routing outcomes.

v0.1 does not automatically retry actions, execute compensation logic, or
claim that external side effects were rolled back. Error records must preserve
the last committed snapshot and state that guest-side external effects may have
occurred.

### R5. Adapter And Extension Boundaries

The Core must define an adapter contract that transforms an artifact into
Host-owned machine IR. Core types must not expose Wasmtime handles, WIT binding
types, or a source file format.

The first adapter loads a WASM Component. It must:

- obtain and validate the complete machine definition from the Component;
- resolve logical guard/action/callback identifiers;
- reject duplicate IDs, missing references, invalid targets, and invalid
  initial/terminal definitions before execution; and
- leave the Host, not guest code, in control of transition execution.

Adapter responsibilities remain distinct from action-function/plugin
responsibilities. Definitions identify executable functions by an action kind
and logical locator. v0.1 resolves the in-component WASM function kind; future
registries may resolve HTTP, shell, remote, or other plugin kinds. A later
JSON/TOML adapter must be able to produce the same Host IR without changing the
Core execution model.

### R6. WASM Component Contract

A v0.1 machine is one self-contained WASM Component containing its definition
and every referenced guard, action, and callback. Separately deployed action
Components and cross-component dependency resolution are deferred.

The canonical WIT contract must use typed records, variants, results, and
logical identifiers for framework control data. Application values use one
payload envelope containing:

- raw bytes;
- a content type; and
- an optional schema identifier.

JSON is the required v0.1 payload encoding, but the Core treats application
payloads as opaque bytes and does not implement JSON-specific patch semantics.
Future codecs or machine/plugin-specific typed WIT interfaces are optional
extensions to, not replacements for, the core ABI.

The official Rust guest SDK and example target `wasm32-wasip2`, which emits a
Component directly. Ordinary Rust `std` builds for this target declare standard
WASI 0.2 imports even when guest business code does not call WASI explicitly.
The v0.1 Host therefore registers Wasmtime's standard WASI interfaces and uses
a minimally configured default context that does not explicitly inherit Host
directories, environment variables, command-line arguments, or networking.
Unsupported non-WASI imports and unsupported WASI interfaces still fail during
preparation with a structured error. The Host must not ignore missing imports
or replace them with trap stubs.

The runtime does not require artifacts to have been produced by a specific
compiler target. A Component built through `wasm32-unknown-unknown`, another
language, or another compatible toolchain may run if its final interface
implements the canonical Shiroha world and all declared imports can be
satisfied by the active Host profile.

Per-task WASI grants, configurable capability profiles, and task-creation
authorization remain pre-v1 work. The v0.1 baseline must keep import discovery
and context construction behind explicit runtime boundaries so that the later
policy layer can restrict or grant capabilities without changing Core FSM
semantics.

Pre-v1 WIT and IR may change incompatibly. The runtime must still wrap
structural Component/interface mismatches in a clear load-time error when the
underlying runtime exposes enough information.

### R7. Guest Authoring Experience

The repository must provide:

- one canonical language-neutral WIT package;
- an official Rust `shiroha-guest` SDK wrapping generated bindings and common
  definition/input/output types; and
- one buildable Rust example Component reused as an end-to-end fixture.

The WASIp2 guest profile and its toolchain-generated WASI imports must be proven
against the v0.1 Host linker before the SDK surface is committed. Official guest
SDKs for other languages are deferred, although those languages may consume the
WIT directly.

### R8. Runtime Safety

Every guest call and run must have configurable, finite hard limits for:

- one enforced CPU/interruption budget: low-overhead epoch deadline by default,
  or deterministic fuel budgeting when explicitly selected;
- linear-memory growth and runtime resource counts;
- payload size; and
- run-to-completion microsteps.

Limits must never become unlimited implicitly. Exhaustion returns a structured
`ResourceLimitExceeded` runtime fault, aborts the current step, and preserves
the last committed snapshot.

v0.1 does not implement tenant quotas, distributed budget propagation, dynamic
resource scheduling, or Controller administration for limits. Those production
governance features are later milestones.

### R9. Host Library API And Concurrency

v0.1 is delivered as Rust library APIs and runnable examples, not an installed
local runner CLI.

The public Host API is async-first and must provide documented operations to:

- compile/load and validate a Component;
- create and start a local machine instance with initial context and limits;
- dispatch one external event or Host signal;
- inspect the current committed snapshot; and
- receive a structured run-to-completion report.

One machine instance serializes dispatch and prevents reentrant event
processing. Guest exports may remain synchronous in v0.1. Pure Host transition
selection remains a synchronous internal path that can be tested and benchmarked
without an async runtime.

### R10. Performance And Observability

v0.1 performance work prioritizes the warm path after a Component has been
compiled and validated. The runtime must not recompile the Component or
reparse/revalidate its definition per event.

Benchmarks must separately report:

- Host-only transition selection/execution overhead;
- warm WASM guard/action/callback invocation;
- Component compilation; and
- Component instantiation.

The first representative implementation establishes the fixed-hardware/CI
baseline. Before release, the project records an allowed regression threshold
instead of inventing an unmeasured absolute target.

Core operations must emit structured `tracing` spans for loading, validation,
startup, dispatch, transition steps, guest calls, unhandled events, and faults.
OpenTelemetry integration belongs to the later Controller milestone and should
consume these spans rather than duplicate instrumentation.

### R11. Pre-v1 Platform Roadmap

The architecture must leave clear boundaries for these later deliverables:

- a controller-owned distributed scheduler that sends selected actions to
  stateless nodes and aggregates structured results;
- a central Controller that owns global task/workflow state, exposes software
  and web APIs, performs security checks, manages tasks, and integrates
  OpenTelemetry;
- stateless Nodes that only execute Controller-issued work and return results;
- Cargo features `full`, `controller`, and `node` controlling role scope;
- an `sctl` CLI that operates exclusively through the Controller API;
- text definition adapters;
- per-task WASI authorization and configurable capability policy; and
- plugins for middleware, aggregation, action functions, and communication
  protocols.

These boundaries must not require v0.1 to implement placeholder services or
public APIs that cannot yet be tested end to end.

## Acceptance Criteria

- [ ] **AC1:** The canonical WIT, Rust guest SDK, and official
      `wasm32-wasip2` example Component build together using a documented
      reproducible pipeline; inspection records its toolchain-generated imports,
      and the baseline Wasmtime WASI linker instantiates it successfully.
- [ ] **AC2:** Loading the example produces Host-owned IR and rejects malformed
      definitions, duplicate IDs, missing functions, invalid targets, and
      incompatible Component shapes before execution. Separate negative tests
      prove that unsupported WASI and non-WASI imports are rejected at load
      time rather than ignored or replaced by trap stubs.
- [ ] **AC3:** Startup and normal/self-transition tests prove the fixed callback
      order and atomic commit behavior.
- [ ] **AC4:** Guard ordering, normal targets, action failure targets, terminal
      outcomes, timeout/cancellation inputs, and distinct business/runtime
      errors have focused tests.
- [ ] **AC5:** Run-to-completion drains internal events FIFO, enforces the
      microstep limit, and returns observable non-fatal `UnhandledEvent`
      outcomes without state/context mutation.
- [ ] **AC6:** Hook context replacements and emitted events remain staged until
      commit; every failure path preserves the last committed snapshot.
- [ ] **AC7:** Destroying and recreating the guest instance between committed
      steps does not lose authoritative task state or change expected results.
- [ ] **AC8:** JSON payload envelopes round-trip bytes, content type, and schema
      ID without Core interpretation or codec-specific patching.
- [ ] **AC9:** Infinite loops, memory growth, oversized payloads, deadlines,
      optional fuel exhaustion, and excessive microsteps are stopped and
      classified as structured resource-limit faults. Tests cover both the
      default epoch mode and the optional deterministic fuel mode.
- [ ] **AC10:** The adapter and execution traits have Core-only tests proving
      that a future non-WASM adapter/executor can use the same IR without a
      Wasmtime dependency.
- [ ] **AC11:** The async Rust example loads, starts, dispatches, and reports a
      local machine while one instance rejects or serializes reentrant dispatch.
- [ ] **AC12:** Tracing and benchmark coverage distinguishes Host-only, warm
      guest, compilation, and instantiation paths; the v0.1 baseline and
      regression threshold are recorded before release.
- [ ] **AC13:** Documentation states the v0.1 scope, pre-v1 Host/Component
      same-release requirement, baseline WASI behavior, deferred capability
      policy/plugin/distributed features, and the route from v0.1 to v1.0.

## Out Of Scope For v0.1

- JSON/TOML definition adapter implementation.
- Per-task WASI authorization, configurable capability grants, and full
  capability-policy enforcement. The minimal v0.1 WASI Host profile is in
  scope only to support ordinary `wasm32-wasip2` Rust Components.
- Official non-Rust guest SDKs.
- Separately deployed action/callback Components.
- Dynamic plugin loading, marketplaces, or hot reload.
- Built-in HTTP, shell, or other external action catalogs.
- Hierarchical states, parallel regions, history, eventless transitions, or
  internal-transition mode.
- Automatic retry, compensation, or guest-memory snapshots.
- Cross-version pre-v1 ABI/IR adapters or automatic migration.
- Distributed scheduling, aggregation across Nodes, Controller/Node services,
  `full`/`controller`/`node` feature builds, and `sctl`.
- Multi-controller consensus, failover, or operation.
- An installed local runner CLI.
