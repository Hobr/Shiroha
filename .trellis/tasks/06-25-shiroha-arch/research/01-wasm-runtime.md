# Research: WASM Runtime + Component Model Library

- **Query**: Evaluate Rust WASM runtime candidates for Shiroha's Component Model adapter (typed `define() -> StateMachineDef`, per-action typed exports, host-func Linker for plugin capabilities, sandbox, async, maturity).
- **Scope**: external (Rust crate ecosystem) — verified via docs.rs as of 2026-06-25
- **Date**: 2026-06-25

## Findings

### Candidates Considered

| Crate | Version | License | Maintainer | Component Model | Typed bindings (WIT→Rust) | Async CM |
|---|---|---|---|---|---|---|
| `wasmtime` | 46.0.1 | Apache-2.0 WITH LLVM-exception | Bytecode Alliance (team, very active) | **Yes — first-class** `wasmtime::component` (`component-model` feature, on by default) | **Yes — `bindgen!` macro** generates typed Rust types + host traits from a WIT world | **Yes** — `component-model-async` feature |
| `wasm_component_layer` | 0.1.18 | Apache-2.0 | Single author (DouglasDwyer) | Yes (runtime-agnostic over `wasm_runtime_layer`) | **No** — "A macro for generating host bindings" explicitly listed as *not yet implemented*; manual `Value`-based dispatch only | **No** (no async component-model surface) |
| `wasmer` | 7.1.0 | MIT | Wasmer team | **No stable CM API** — `wasmer` crate is core-wasm only (Module/Instance/TypedFunction); CM/WIT not exposed in the stable embedding API | N/A | N/A |

### Assessment against Shiroha's needs (a–f)

#### wasmtime 46.0.1 — the canonical Component Model embedding API

**(a) Call typed `define() -> StateMachineDef` + deserialize typed records** ✅
`wasmtime::component::bindgen!` consumes a WIT `world` and generates Rust types for every WIT `record`/`variant`/`enum`/`list`, plus a trait per exported interface. `Component::new(&engine, bytes)` → `Linker::instantiate` → `instance.get_typed_func::<(), StateMachineDef>(&mut store, "define")` gives a `TypedFunc<Params, Return>` with native Rust types. `ComponentType`/`Lift`/`Lower` derives exist for custom host types. This is *exactly* the "call `define()`, get a typed `StateMachineDef` record" flow.

**(b) Per-action named exports with typed args/returns** ✅
Each action is just another WIT `export func`. `bindgen!` generates one typed method per export; or at runtime `instance.get_func(&mut store, "<action-name>")` → `.typed::<Input, Output>()` for dynamic per-action dispatch by name (needed since action names are data, not statically known). Both static (bindgen) and dynamic (`Func`/`TypedFunc` by name) paths are supported.

**(c) Host funcs via Linker for capabilities (WASI worlds + framework-native interfaces; capability whitelist injection)** ✅
`wasmtime::component::Linker` + `LinkerInstance::func_wrap[_async]` register host funcs under `(interface, name)`. The host builds one `Linker` per capability-whitelist policy and instantiates the component against it. Imports declared in WIT become required host-func slots the host must fill (or instantiation fails) → natural capability negotiation surface. `Resource<T>` / `ResourceTable` available for handle-style capabilities. **Capability model (revised, see prd D7/R3.5)**: capabilities = all host-provided imports the wasm component declares = standard WASI worlds (`wasi:io`/`clocks`/`filesystem`/`sockets`/`http`) + framework-native interfaces (`shiroha:shell`/`shiroha:log` for things WASI can't express). `http`/`fs` are WASI/`shiroha:*` capabilities, **not plugins**. Plugins (`ActionRef::Plugin{plugin_id, method}`) are a separate action-layer extension axis (wasm or host-native), orthogonal to capabilities. Full task-creation-time capability authorization is a v0.10 feature; MVP (v0.2/v0.4) keeps a minimal directly-wired host-func channel.

**(d) Sandbox: fuel/cycles + memory cap + timeout/epoch** ✅ (best-in-class)
- `Config::consume_fuel(true)` + `Store::set_fuel` / `Store::fuel_async_yield_interval` — deterministic per-op cost budget.
- `Config::epoch_interruption(true)` + `Store::set_epoch_deadline` + `Engine::increment_epoch` — lightweight cooperative interruption; docs state ~2–3× faster than fuel, and "deadline check cannot be avoided by malicious wasm code." For async: `Store::epoch_deadline_async_yield_and_update` yields the future back to the executor periodically (cooperative timeslicing).
- Memory/stack caps: `Config::max_wasm_stack`, `Config::memory_reservation` / `memory_reservation_for_growth` / `memory_guard_size`, plus `StoreLimits` / `StoreLimitsBuilder` per-store (memories/tables/instances cap) and `PoolingAllocationConfig` (pooling-allocator feature, on by default) for bounded concurrent resource use.
- Timeout: combine epoch deadline with `tokio::time::timeout` on the driving future (recommended pattern in wasmtime docs).

**(e) Async (tokio integration) since actions are async** ✅
`async` feature is **on by default**. `Func::call_async` / `TypedFunc::call_async`; `Linker::func_wrap_async` for async host funcs (the plugin channel). Component-model async (WIT `async` funcs, `future`/`stream` types) under `component-model-async` feature: `FutureReader`, `StreamReader`, `Func::start_call_concurrent`, `Accessor` for concurrent host-task futures, `JoinHandle`, `GuestTaskId`. **Critical nuance:** wasmtime async is **runtime-agnostic** — it represents guest computation as a Rust `Future` executed on a separately allocated native stack (fiber-based stack switching) and "won't manage its own thread pools… left up to the embedder." tokio is only a *dev*-dependency. So wasmtime does **not** hard-require tokio — but it composes perfectly with it (and tonic forces tokio anyway; see `02-async-runtime.md`).

**(f) Maturity / license / maintenance / Rust requirements** ✅
Apache-2.0 WITH LLVM-exception (permissive, GPL-compatible). Bytecode Alliance, very active, MSRV rolling ~recent stable. Cranelift JIT (default) + Winch baseline + Pulley interpreter. This is the reference implementation of the Component Model — the CM spec and wasmtime co-evolve.

#### wasm_component_layer 0.1.18 — runtime-agnostic, but not viable for Shiroha

- **(a)/(b) FAILING**: no `bindgen!`-equivalent (explicitly unimplemented). Calling `define()` and per-action exports means manual `Value`/`ValueType` dynamic dispatch — you lose the typed `StateMachineDef` record deserialization that is the whole point of the CM adapter. Error-prone and verbose.
- **(c)**: `Linker` exists; host funcs possible but without typed bindings the ergonomics are poor.
- **(d)**: depends entirely on the backend runtime's sandbox (`wasm_runtime_layer`); wasmi backend has no fuel/epoch equivalent matching wasmtime's.
- **(e)**: **no async Component Model surface** — incompatible with Shiroha's async-action requirement.
- **(f) RISK**: single maintainer (bus factor); dependency versions are *years* behind (`wasmtime-environ ^18`, `wit-component ^0.19`, `wit-parser ^0.13` while wasmtime is at 46 / wasm-tools at 0.251) → lagging the CM spec. 100% doc-coverage doesn't offset the staleness.

#### wasmer 7.1.0 — ruled out

Stable `wasmer` crate exposes only **core WebAssembly** (`Module`, `Instance`, `Function`, `TypedFunction`, `Memory`, `imports!`). There is **no `component` module, no WIT, no `bindgen!`, no Component Model records** in the stable API. wasmer's CM support has historically been experimental/WASP and is not part of the documented embedding API. Additionally builds via C/CMake/bindgen (heavier than wasmtime's pure-Rust-ish Cranelift path). Not suitable for a CM-based adapter.

### Recommendation

**Use `wasmtime` 46.x** as the WASM runtime + Component Model library. It is the reference implementation of the Component Model and the only candidate that satisfies *all six* of Shiroha's needs out of the box: typed `bindgen!` for `define() -> StateMachineDef` and per-action exports, `component::Linker` for whitelisted host-func capability injection, the strongest sandbox toolkit in the ecosystem (fuel + epoch + `StoreLimits` + pooling allocator), full async (core + component-model-async) with tokio composition, permissive license, and active Bytecode Alliance maintenance.

Enable features: `component-model` (on by default), `async` (on by default), `component-model-async`, `cranelift` (on by default), `pooling-allocator` (on by default), `cache`. Keep `call-hook` off unless you wire `Store::call_hook` for observability.

**Runner-up**: `wasm_component_layer` — *only* if a future hard requirement emerges for a non-wasmtime backend (e.g. embedding under wasmi for a no-JIT/constrained target, or `wasm_runtime_layer` portability). Acceptable for a "tiny interpreter" deployment mode, but today it lacks typed bindings and async CM, so it would force a manual/dynamic dispatch layer and an async shim. Treat as a possible future *secondary* backend behind a trait, not the primary.

### Concrete API sketch (host loads component, calls `define()`, pre-links actions)

```rust
// WIT world:
//   world shiroha-machine {
//     import wasi:filesystem/preopens;   // WASI world (capability)
//     import shiroha:log/emit;           // framework-native interface (capability)
//     export define: func() -> machine-def;          // typed record (incl. capabilities declaration)
//     export action-<name>: func(input: list<u8>) -> result<list<u8>, string>;  // per-action
//   }
wasmtime::component::bindgen!({
    path: "shiroha.wit",
    world: "shiroha-machine",
    async: true,                 // generate call_async + concurrent host-func traits
});

let mut cfg = wasmtime::Config::new();
cfg.wasm_component_model(true)
   .wasm_component_model_async(true)
   .consume_fuel(true).epoch_interruption(true)
   .max_wasm_stack(512 * 1024)
   .memory_reservation(64 * 1024 * 1024);
let engine = wasmtime::Engine::new(&cfg)?;

let component = wasmtime::component::Component::from_file(&engine, "machine.wasm")?;
let mut linker = shiroha_machine::Host::new_linker(&engine, &host_state)?; // whitelisted WASI + shiroha:* host funcs
// (host_state implements only the capabilities this task is authorized for; others are not registered)
let (mut store, instance) = shiroha_machine::ShirohaMachine::instantiate_async(
    &mut Store::new(&engine, store_data), &mut linker, &component).await?;

let machine_def = instance.call_define(&mut store).await?;   // -> typed MachineDef (== SmIr source)
// pre-link per-action func refs by name for the engine:
let act = store.get_func(&instance, "action-validate")
    .and_then(|f| f.typed::<Vec<u8>, Result<Vec<u8>, String>>(&store))?;
// engine invokes `act.call_async(...)` when the action fires; WASI/shiroha:* host funcs inside = capability channel (plugin is a separate action-layer axis)
```

### Risks / Caveats

- **Component Model + async is the newest part of wasmtime** (`component-model-async`). It is usable but evolving; track wasmtime releases and pin an exact minor to avoid lift/lower ABI churn.
- **`bindgen!` generates code at compile time from a WIT world** — but Shiroha's per-action export names are *data* (defined per machine). Strategy: generate the typed `define()` + the host-capability interfaces with `bindgen!`, and resolve per-action funcs **dynamically** at runtime via `instance.get_func(name).typed::<In, Out>()` with a fixed canonical action ABI (`list<u8> -> result<list<u8>, string>` or a richer shared `ActionResult` record). Do not try to `bindgen!` every possible action name.
- **Epoch interruption needs a driver**: a periodic `Engine::increment_epoch()` (tokio interval) and per-call `tokio::time::timeout` on the driving future. Without it, an infinite loop in a wasm action traps only on fuel (if enabled).
- **Store is not `Sync`**; one `Store` per state-machine instance (fits the multi-instance model: `Store<T>` per task, `Engine` shared).
- Verify the exact `bindgen!` options for *concurrent* async host funcs (`Accessor`/`HasData`) before pinning — this API was refined across 3x→6x.

## External References

- [wasmtime::component module docs (46.0.1)](https://docs.rs/wasmtime/latest/wasmtime/component/index.html) — Component Model embedding API, `bindgen!`, `Component`, `Linker`, `TypedFunc`.
- [wasmtime::Config (46.0.1)](https://docs.rs/wasmtime/latest/wasmtime/struct.Config.html) — `consume_fuel`, `epoch_interruption`, `max_wasm_stack`, `memory_reservation`, `wasm_component_model_async`.
- [wasmtime crate docs (46.0.1)](https://docs.rs/wasmtime/latest/wasmtime/index.html) — Async section (fiber-based, runtime-agnostic), crate features list.
- [Component Model book](https://component-model.bytecodealliance.org) / [WIT design](https://component-model.bytecodealliance.org/design/wit.html).
- [wasm_component_layer docs (0.1.18)](https://docs.rs/wasm_component_layer/latest/wasm_component_layer/) — "A macro for generating host bindings" listed under unimplemented features.
- [wasmer docs (7.1.0)](https://docs.rs/wasmer/latest/wasmer/) — core-wasm-only API; no `component` module.

## Related Specs

- `.trellis/spec/` — none yet (this research feeds R6.1 selection in `design.md`).
