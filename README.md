# Shiroha

Shiroha is a WebAssembly-extensible workflow runtime built around deterministic
finite-state machines. The v0.1 line is a local Rust library: a WASM Component
defines a machine and implements its guards, actions, and callbacks, while the
Host owns execution order, committed state, event queues, validation, resource
limits, and diagnostics.

## v0.1 Scope

Implemented in this repository:

- a flat event-driven FSM with one active state;
- ordered guards and fixed exit → action → entry lifecycle semantics;
- atomic Host-owned context/state commits and FIFO internal events;
- normal and business-failure targets;
- logical timeout/cancellation inputs and observable unhandled events;
- a runtime-neutral Core with adapter and executor boundaries;
- a Wasmtime Component Model adapter using typed WIT calls;
- a canonical WIT package, Rust guest SDK, and WASIp2 example Component;
- finite epoch/fuel, wall-time, memory, payload, event, and microstep limits;
- structured `tracing` spans; and
- async Rust facade APIs and a runnable example.

The Controller, stateless Nodes, distributed scheduler, `sctl`, text adapters,
dynamic plugins, task authorization, and configurable capability policy are
pre-v1 milestones, not v0.1 placeholder APIs.

## Architecture

```text
Application
    ↓
shiroha facade
    ├── shiroha-core             Host IR, validation, FSM engine
    └── shiroha-adapter-wasm     Wasmtime loader and guest executor
            ↓
      WASM Component
      ├── machine definition
      ├── guards
      ├── actions
      └── callbacks
```

The guest never runs the state-machine loop. Guest memory may be reused for the
warm path, but it is disposable and is not authoritative workflow state.

## Host Usage

```rust,no_run
use shiroha::core::{HostInput, PayloadEnvelope};
use shiroha::{Event, EventName, ShirohaRuntime};

# async fn run(component: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
let runtime = ShirohaRuntime::builder().build()?;
let prepared = runtime.prepare_component(component).await?;
let mut machine = prepared
    .start(PayloadEnvelope::json(br#"{"phase":"idle"}"#.to_vec()))
    .await?;

let report = machine
    .dispatch(HostInput::Event(Event::new(
        EventName::new("begin")?,
        None,
    )))
    .await?;

println!("outcome: {:?}", report.outcome);
println!("snapshot: {:?}", machine.snapshot());
# Ok(())
# }
```

`LocalMachine::dispatch` requires `&mut self`, so one instance cannot process
reentrant or concurrent events without an application-owned actor/mutex
boundary.

## Guest Components

The official Rust guest target is `wasm32-wasip2`. The canonical contract is
[`wit/shiroha-machine/world.wit`](wit/shiroha-machine/world.wit), and
`shiroha-guest` provides generated types, a `MachineGuest` trait, helpers, and
the `export_machine!` macro.

Build and inspect the example:

```bash
just build-example
just validate-example
```

The resulting artifact is
`target/components/wasm32-wasip2/debug/example_machine.wasm`.

### Baseline WASI Profile

Ordinary Rust `std` Components built for `wasm32-wasip2` declare standard WASI
0.2 imports even when business code does not explicitly use WASI. v0.1 links
those standard interfaces through `wasmtime-wasi`.

Each Store starts from `WasiCtxBuilder::new()`:

- stdin is closed and stdout/stderr are sinks;
- no Host environment variables or arguments are inherited;
- no filesystem directories are preopened; and
- socket addresses and name lookup are denied by default.

The allowlist mirrors the exact stable Preview 2 interfaces registered by the
pinned Wasmtime 46.0.1 linker through version 0.2.12. Unknown interfaces inside
a recognized WASI family, newer unsupported patches, and non-WASI imports are
rejected before machine interface loading. Per-task grants and authorization
will replace this fixed baseline with a configurable capability policy before
v1.0.

## Finite Defaults

| Limit | v0.1 default |
|---|---:|
| CPU mode | Epoch interruption |
| Epoch budget | 100 ticks at a 10 ms process ticker, capped by wall time |
| Guest wall time per call | 1 second |
| Deterministic fuel mode | Configurable, finite units |
| Linear memory per Store | 64 MiB |
| Payload data | 1 MiB |
| Payload content type / schema ID | 4 KiB each |
| Events emitted per hook | 256 |
| Run-to-completion microsteps | 1,024 |

Selecting fuel mode builds a fuel-enabled Wasmtime Engine; the default Engine
uses epoch interruption. Limit exhaustion is reported as a structured runtime
fault and staged state/context/events are discarded.

The first measured warm-path reference and regression policy are recorded in
[`docs/benchmarks/v0.1-baseline.md`](docs/benchmarks/v0.1-baseline.md).

## Development

The repository requires Rust 1.97.0 and installs the `wasm32-wasip2` target.

```bash
nix develop
just install-dev
just check
just build-example
just test
just fmt
```

`just install-dev` pins Wasmtime CLI 46.0.1 and wasm-tools 1.253.0 to match the
validated Component pipeline.

## Compatibility And Roadmap

v0.x Host IR and WIT may change incompatibly. Until v1.0, build the Host and
Components from the same Shiroha release/revision; there is no automatic ABI or
snapshot migration promise.

Planned before v1.0:

1. text definition adapters and plugin registries;
2. Controller-owned task state, APIs, security checks, and OpenTelemetry;
3. stateless execution Nodes and distributed action scheduling/aggregation;
4. Cargo roles `full`, `controller`, and `node` plus the `sctl` client; and
5. task-creation authorization with configurable WASI capability grants.

Multi-controller consensus/failover is intentionally deferred until after the
production-ready v1.0 release.
