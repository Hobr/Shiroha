# Research Index — Shiroha Phase 1.2 Technology Selection

- **Task**: `.trellis/tasks/06-25-shiroha-arch` (parent architecture task)
- **Phase**: 1.2 research (feeds R6.1–R6.5 + AC2/AC5/AC6/AC7 in `design.md`)
- **Date**: 2026-06-25
- **Method**: external verification via docs.rs (versions/licenses/features) + architectural analysis against the already-decided Shiroha product model. No code modified.

## Conclusions Table

| # | Topic | Chosen crate / approach | One-line rationale | Runner-up | File |
|---|---|---|---|---|---|
| 1 | WASM runtime + Component Model | **`wasmtime` 46.x** (features `component-model` + `component-model-async` + `async` + `cranelift` + `pooling-allocator`) | Only candidate satisfying all 6 needs: typed `bindgen!` for `define()->StateMachineDef` + per-action exports, `component::Linker` for whitelisted host-func capabilities, best sandbox (fuel + epoch + `StoreLimits` + pooling), full async, Apache-2.0/LLVM, Bytecode Alliance. | `wasm_component_layer` (only if a non-wasmtime backend is later required; lacks typed bindings + async CM today) | [01-wasm-runtime.md](01-wasm-runtime.md) |
| 2 | Async runtime | **`tokio`** (`rt-multi-thread`+`macros`+`time`+`signal`) | tonic (transport) hard-requires tokio; wasmtime async is runtime-agnostic but composes perfectly with tokio, which also drives epoch interrupts + OTLP export. | none practical (smol/async-std incompatible with tonic) | [02-async-runtime.md](02-async-runtime.md) |
| 3 | Serialization / IR unification | **`serde` + `serde_json` + `serde-saphyr` (YAML) + `toml`**; one canonical serde-derived `SmIr`; WASM CM `bindgen!`-generated `MachineDef` + `From<MachineDef> for SmIr` | Single `SmIr` is the engine contract; text adapters = thin serde calls, CM adapter = typed record + trivial conversion. ⚠️ `serde_yaml` AND `serde_yml` are deprecated → use `serde-saphyr`. | `noyalib` (YAML drop-in) ; fold `shiroha-ir` into `shiroha-core` (coarser layout) | [03-serialization-ir.md](03-serialization-ir.md) |
| 4 | Transport | **`tonic` 0.14 + `prost` 0.14** (default gRPC `Transport`); abstract `Transport` trait in `shiroha-transport` (prost-free) | Bidi `Dispatch` streaming RPC = orchestrator↔worker action dispatch + result回流; trait keeps libp2p/QUIC swappable; TLS via tonic rustls features. | raw QUIC (`quinn`) / libp2p (implement same trait) | [04-transport.md](04-transport.md) |
| 5 | Observability | **`tracing` 0.1** everywhere + **`opentelemetry` 0.32 family + `tracing-opentelemetry` 0.33 + `opentelemetry-appender-tracing`** in one `shiroha-otel` crate | Standard Rust OTel stack; per-task spans with trace-context propagation over gRPC. ⚠️ version-locked at 0.32; traces stable, metrics not fully stable, logs bridge experimental. | swap `opentelemetry-otlp` for `-stdout`/`-prometheus` | [05-observability.md](05-observability.md) |
| 6 | Workspace layout | **12-crate Cargo workspace**: `shiroha-ir` (serde-only leaf) → `shiroha-core` (pure engine) → adapters / `plugin-sdk` / `transport`(+grpc) / `scheduler` / `worker` / `otel` / `controller` → `shiroha` facade + 2 binaries | Layer = crate boundary; core depends on nothing upstream; wasmtime confined to 2 crates, tonic to 2, OTel to 1; pluggable boundaries = trait crate + default impl crate. | fold `ir` into `core` (fewer crates, loses "text adapters avoid engine") | [06-workspace-layout.md](06-workspace-layout.md) |

## Critical caveats the main agent must know now

1. **YAML crate land-grab is mid-migration (verified 2026-06-25):** `serde_yaml` (dtolnay-archived) and `serde_yml` (now a deprecated shim) are BOTH unmaintained. Use **`serde-saphyr`** (typed deser, no `Value` DOM — fits `from_str::<SmIr>`). Isolate behind a `YamlAdapter` type; re-verify version before pinning. Do NOT start a YAML adapter against `serde_yaml`/`serde_yml`.
2. **OpenTelemetry version lockstep at 0.32:** `tracing-opentelemetry 0.33` pins `opentelemetry ^0.32`. All `opentelemetry*` crates + `-appender-tracing` + `-semantic-conventions` must be the **same 0.32.x**. Bump them together. Traces stable; **metrics not fully stable**; logs bridge experimental. Confine to `shiroha-otel`; instrument with `tracing` only elsewhere.
3. **wasmtime async is runtime-agnostic but tokio is forced by tonic** — standardize on tokio everywhere (no smol/async-std). wasmtime `Store` is `!Sync` → one `Store` per state-machine instance; design the engine loop around that (per-instance `LocalSet` or `Arc<Mutex<Store>`).
4. **`bindgen!` is compile-time from a WIT world, but Shiroha's per-action export names are data.** Generate `define()` + host-capability interfaces with `bindgen!`; resolve per-action funcs **dynamically** at runtime via `instance.get_func(name).typed::<In,Out>()` against a fixed canonical action ABI. Don't try to `bindgen!` every action name.
5. **`prost` requires `protoc` at build time** (no longer bundled) — ensure `protoc` is in CI/devshell, or vendor via `protoc-bin-vendored` for hermetic builds. `prost` is "passively maintained" but remains the de-facto tonic codec.
6. **`wasmer` ruled out** for the CM adapter: stable `wasmer` 7.1 API is core-wasm only (no Component Model / WIT / `bindgen!`). `wasm_component_layer` ruled out as primary: single maintainer, stale deps (wasmtime-environ ^18 vs wasmtime 46), **no host-bindings macro**, no async CM.

## Files written

- `research/01-wasm-runtime.md` — wasmtime recommendation + runner-up + API sketch
- `research/02-async-runtime.md` — tokio confirmation + wasmtime-imposed constraints
- `research/03-serialization-ir.md` — serde stack + unified `SmIr` design (text ↔ CM)
- `research/04-transport.md` — tonic/prost + proto sketch + abstract `Transport` trait
- `research/05-observability.md` — OTel 0.32 family + tracing bridge + maturity caveats
- `research/06-workspace-layout.md` — 12-crate workspace DAG + dependency invariants
- `research/index.md` — this file
