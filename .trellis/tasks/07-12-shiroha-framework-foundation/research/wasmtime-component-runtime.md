# Wasmtime 46 Component Runtime Research

## Sources

- [Wasmtime v46.0.1 configuration source](https://github.com/bytecodealliance/wasmtime/blob/v46.0.1/crates/wasmtime/src/config.rs)
- [Wasmtime v46.0.1 store source](https://github.com/bytecodealliance/wasmtime/blob/v46.0.1/crates/wasmtime/src/runtime/store.rs)
- [Wasmtime v46.0.1 resource limits](https://github.com/bytecodealliance/wasmtime/blob/v46.0.1/crates/wasmtime/src/runtime/limits.rs)
- [Wasmtime v46.0.1 typed Component functions](https://github.com/bytecodealliance/wasmtime/blob/v46.0.1/crates/wasmtime/src/runtime/component/func/typed.rs)
- [Wasmtime v46.0.1 Component instances](https://github.com/bytecodealliance/wasmtime/blob/v46.0.1/crates/wasmtime/src/runtime/component/instance.rs)
- [Component Model WIT reference](https://component-model.bytecodealliance.org/design/wit.html)

## Confirmed Capabilities

- Component Model execution is enabled through Wasmtime's Component APIs and
  typed host bindings can be generated from WIT.
- `TypedFunc::call_async` is available with Wasmtime's `async` feature.
- `InstancePre::instantiate_async` supports preparing import resolution before
  repeated instantiation; this fits the warm-path and recreate-anytime design.
- `Config::consume_fuel(true)` plus `Store::set_fuel` provides deterministic
  instruction budgeting.
- `Config::epoch_interruption(true)` plus store epoch deadlines provides
  coarse wall-time interruption. The Wasmtime documentation explicitly
  distinguishes epoch interruption from deterministic fuel accounting.
- `StoreLimitsBuilder` and `Store::limiter` can bound linear-memory size, table
  size, and the numbers of instances/tables/memories.
- Component Model async/concurrency proposals are separate features. Shiroha
  v0.1 does not need async WIT exports: the Host API can be async while invoking
  ordinary synchronous guest exports through Wasmtime's async call path.

## Recommended Runtime Shape

1. Build one Wasmtime `Engine` with Component Model, fuel consumption, epoch
   interruption, and the required async feature enabled.
2. Compile and validate each Component once into a prepared machine artifact.
3. Use a prepared linker/`InstancePre` so task instances do not redo import
   type-checking and name lookup.
4. Create one `Store` per active local machine instance. Store data owns
   `StoreLimits`, invocation metadata, and resource-limit state.
5. Reset fuel and epoch deadline for every guest call. Apply a Tokio deadline as
   an outer safety/reporting boundary while relying on Wasmtime interruption to
   stop guest code.
6. Treat fuel exhaustion, epoch interruption, memory-limit failure, canonical
   ABI violations, and traps as structured runtime faults.
7. Keep Component compilation and instantiation benchmarks separate from warm
   guard/action/callback call benchmarks.

## Risks To Verify During Implementation

- Exact generated binding names and async call signatures must be proven with
  the final WIT world and Wasmtime 46.0.1, not copied from newer docs.
- Epoch ticking needs a process-level owner with deterministic shutdown; a
  leaked background task is unacceptable.
- Fuel and wall-time errors need reliable classification from Wasmtime's error
  chain instead of string matching.
- Resource limits apply at Store scope. The planned one-Store-per-machine model
  must remain true or limit accounting must be redesigned.
