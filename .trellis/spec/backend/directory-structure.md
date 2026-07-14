# Directory Structure

> How Rust runtime code is organized and which dependencies may cross layers.

## Overview

The workspace uses crate boundaries to enforce dependency direction. Business
semantics live in `shiroha-core`; runtime integration lives in adapters; guest
ergonomics live in `shiroha-guest`; applications use the `shiroha` facade.

## Directory Layout

```text
crates/
├── shiroha-core/           # IR, validation, limits, FSM, runtime-neutral traits
├── shiroha-adapter-wasm/   # Wasmtime/WASI bindings, conversion, loader, executor
├── shiroha-guest/          # wit-bindgen exports and Rust guest helpers
└── shiroha/                # supported application-facing facade
components/
└── example-machine/        # nested guest workspace, built for wasm32-wasip2
wit/shiroha-machine/        # canonical Component contract
docs/benchmarks/            # measured performance baselines
docs/testing/               # acceptance-to-test mapping
```

## Dependency Direction

```text
application -> shiroha -> shiroha-adapter-wasm -> shiroha-core
                                      |
                                      +-> wasmtime / wasmtime-wasi

guest Component -> shiroha-guest -> canonical WIT
```

`shiroha-core` must not depend on Wasmtime, WIT-generated types, a concrete
definition format, Controller code, or transport code. Convert external types
at the adapter edge. Keep the example Component excluded from the Host
workspace because its target and crate graph are guest-specific.

## Module Organization

- Add IR data types to `model.rs`, validated IDs to `id.rs`, finite limits to
  `limits.rs`, runtime reports/errors to `runtime.rs`, execution traits to
  `executor.rs`, structural checks/indexes to `validation.rs`, and FSM behavior
  to `engine.rs`.
- In the WASM adapter, keep generated bindings, WIT conversion, public errors,
  runtime/Store policy, invocation, and preparation in separate modules.
- Expose application workflows through the facade rather than teaching callers
  how to compose Core and Wasmtime internals.
- Add a new crate only for a real dependency or deployment boundary with an
  executable consumer; do not scaffold deferred roadmap crates.

## Naming Conventions

- Crates and directories use `shiroha-*` kebab case; Rust modules use snake
  case; public types use PascalCase.
- Adapter crates name the source/runtime explicitly, for example
  `shiroha-adapter-wasm`.
- Tests name the behavior and outcome, such as
  `restore_rejects_a_snapshot_from_another_machine`.

## Reference Implementations

- `crates/shiroha-core/src/engine.rs` for Host-owned atomic execution.
- `crates/shiroha-adapter-wasm/src/loader.rs` for boundary preparation.
- `crates/shiroha/src/lib.rs` for facade composition.
- `components/example-machine/src/lib.rs` for a guest implementation.
