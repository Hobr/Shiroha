# Research: Serialization / Text Adapters / Unified IR

- **Query**: Confirm serde + JSON/YAML/TOML crates for text adapters producing the IR; define how to unify the "text-adapter IR" (serde-deserialized) and the "WASM Component Model typed-record IR" (from `define()`) into one canonical IR type.
- **Scope**: external (verified via docs.rs) + analysis
- **Date**: 2026-06-25

## Findings

### Crate selection (verified)

| Concern | Crate (version) | License | Status |
|---|---|---|---|
| Serialization framework | `serde` 1.0.x (derive) | MIT OR Apache-2.0 | stable, universal |
| JSON | `serde_json` 1.0.x | MIT OR Apache-2.0 | stable |
| YAML | **`serde-saphyr`** (maintained) | (check on pin) | see caveat — YAML ecosystem mid-migration |
| TOML (parse→IR) | `toml` 1.1.2 (spec 1.1.0) | MIT OR Apache-2.0 | stable, serde-compatible |
| TOML (preserve formatting/edit) | `toml_edit` (sibling) | MIT OR Apache-2.0 | use only if a tool needs round-trip editing |

### CRITICAL YAML caveat (verified 2026-06-25)

Both historically-recommended YAML crates are now **deprecated/unmaintained**:
- `serde_yaml` 0.9.34+deprecated — dtolnay archived it ("This project is no longer maintained").
- `serde_yml` 0.0.13 — **also deprecated**; 0.0.13 is a thin compatibility shim forwarding to `noyalib`; docs say migrate to `noyalib` / `serde-saphyr` / `yaml-rust2`.

**Maintained alternatives** (per `serde_yml` migration guide):
- **`serde-saphyr`** — modern parser, serde-integrated **typed** deserialization; *no* `Value` DOM. Fits codebases that only call `from_str::<MyStruct>`. ← **best fit for Shiroha** (we deserialize YAML directly into `SmIr`).
- `noyalib` — pure-Rust `#![forbid(unsafe_code)]`, drop-in via `compat-serde-yaml` feature (0.0.x, very new).
- `yaml-rust2` — low-level parser primitives, no serde wrapper.

**Recommendation**: use `serde-saphyr` for the YAML adapter (`serde_saphyr::from_str::<SmIr>(...)`). Flag: the YAML crate landscape is actively churning — re-verify the exact crate name + version at implementation time and keep the YAML adapter behind a thin `YamlAdapter` type so the crate can be swapped with one import change. JSON (`serde_json`) and TOML (`toml`) are stable and non-controversial.

### Unifying text-adapter IR and WASM CM typed-record IR

**Goal**: one canonical `SmIr` type that (a) deserializes from JSON/YAML/TOML via serde, and (b) is constructable from the typed Component Model record returned by `define()`.

**Recommended approach — canonical `SmIr` serde struct + a from-CM conversion:**

1. **Define `SmIr` once**, in `shiroha-ir` (or `shiroha-core`), as plain serde-derived Rust structs/ enums. This is the *only* type the engine consumes. It is **runtime-agnostic and serialization-backend-agnostic**.

```rust
// shiroha-ir / the single canonical IR the engine eats
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SmIr {
    pub name: String,
    pub root: StateRef,
    pub states: Vec<StateNode>,
    pub transitions: Vec<Transition>,
    pub actions: Vec<ActionRef>,     // unified {kind, ref}
    pub history: Vec<HistoryDecl>,   // shallow history
    pub capabilities: Vec<CapabilityDecl>, // WASI worlds + shiroha:* interface declarations
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionRef {
    WasmFunc  { export: String },                       // {wasm-func, <component-export-name>}
    Plugin    { plugin_id: String, method: String },    // {plugin, <id>.<method>} (wasm or host-native, opaque to caller)
    Distributed { inner: Box<ActionRef>, fanout: Option<u32>, target: Option<TargetSpec>, aggregate: AggregateRef }, // R4
}
// capability = wasm component's host-provided imports (orthogonal to plugin)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapabilityDecl { pub interface: String, pub functions: Vec<String> }
// StateNode / Transition encode nested + parallel regions, guards, entry/exit/run actions…
```

> **Capability/plugin separation (prd D7/R3.5)**: `http`/`fs` are WASI/`shiroha:*` capabilities (not plugins); `shell`/`log` are framework-native `shiroha:*` capabilities. Plugins are purely an action/aggregator extension axis. Full task-creation-time capability authorization is a v0.10 feature; MVP keeps a minimal host-func channel.

2. **Text adapters** are thin: each just calls the right serde backend.
```rust
pub fn from_json(s: &str) -> Result<SmIr, _> { serde_json::from_str(s) }
pub fn from_yaml(s: &str) -> Result<SmIr, _> { serde_saphyr::from_str(s) } // or chosen YAML crate
pub fn from_toml(s: &str) -> Result<SmIr, _> { toml::from_str(s) }
```
Because `SmIr` is serde-derived, all three backends produce the *same* `SmIr` with no per-backend code.

3. **WASM CM adapter** uses `wasmtime::component::bindgen!` to generate a *typed* Rust mirror of the WIT `machine-def` record (call it `MachineDefComponent` — generated, CM-canonical-ABI types). The host calls `instance.call_define()` → gets `MachineDefComponent`. Then a single **`From<&MachineDefComponent> for SmIr`** (or `TryFrom`) conversion collapses the generated CM types into the canonical `SmIr`:
```rust
impl From<shiroha_machine::MachineDef> for SmIr {
    fn from(c: shiroha_machine::MachineDef) -> SmIr {
        SmIr { name: c.name, states: c.states.into_iter().map(Into::into).collect(), /* … */ }
    }
}
```
The CM types and `SmIr` are *structurally identical by construction* (the WIT world is authored to mirror `SmIr`'s shape), so the conversion is field-by-field `Into::into` — no semantic translation, just ABI type mapping (CM `list<T>` → `Vec<T>`, CM `option<T>` → `Option<T>`, CM `result<T,E>` → `Result<T,E>`, CM `record` → struct, CM `variant` → enum).

4. **Engine consumes only `SmIr`.** Both adapter families converge before the engine boundary:
```
text ──serde──> SmIr ──┐
                        ├──> Engine
wasm ──bindgen──> MachineDefComponent ──From──> SmIr ──┘
```

### Why this split (and not "make `SmIr` itself a CM type")

- serde-derived structs are **not** automatically `ComponentType`/`Lift`/`Lower`. Trying to make one struct serve both serde *and* the CM canonical ABI couples `SmIr` to wasmtime's ABI derives and to WIT's type system (no `serde` attrs on CM types, naming constraints, etc.). Keeping them separate with a trivial `From` is cleaner and lets the text path stay wasmtime-free (so `shiroha-adapter-text` need not depend on wasmtime).
- The generated `MachineDefComponent` is *free* (from `bindgen!`); writing the `From` impl is mechanical and the natural place to validate CM-specific invariants before entering the canonical IR.
- `SmIr` stays the single engine contract (AC2: "adapter↔core IR contract defined"). New adapters (e.g. a future SCXML adapter) only need to produce `SmIr`.

### Recommendation

- **`serde` + `serde_json` + `serde-saphyr` (YAML) + `toml`** for text adapters.
- **One canonical `SmIr`** (serde-derived) in `shiroha-ir`, consumed only by the engine.
- **`bindgen!`-generated `MachineDefComponent` + `From<MachineDefComponent> for SmIr`** for the WASM CM adapter.
- Keep `shiroha-adapter-text` free of any wasmtime dependency; keep `shiroha-adapter-wasm` as the only crate that depends on wasmtime + `SmIr`.

**Runner-up (YAML only)**: `noyalib` with `compat-serde-yaml` if a drop-in `serde_yaml`-shaped API is preferred; avoid `serde_yaml`/`serde_yml` (deprecated).

### Risks / Caveats

- **YAML crate churn** — pin `serde-saphyr` only after a fresh version check; isolate behind `YamlAdapter` so a swap is one import.
- `toml` cannot represent deeply-nested homogeneous maps as ergonomically as JSON/YAML; confirm the `SmIr` shape is TOML-friendly (use `[states.<id>]` tables) or accept that TOML is best for *small* machine definitions.
- The `From<MachineDefComponent> for SmIr` impl must be regenerated/updated whenever the WIT world changes — keep WIT and `SmIr` in lockstep by construction (single source spec).

## External References

- [serde_yaml 0.9.34+deprecated](https://docs.rs/serde_yaml/latest/serde_yaml/) — "no longer maintained".
- [serde_yml 0.0.13](https://docs.rs/serde_yml/latest/serde_yml/) — deprecated shim; migration guide lists `serde-saphyr` / `noyalib` / `yaml-rust2`.
- [toml 1.1.2](https://docs.rs/toml/latest/toml/) — serde-compatible, `from_str`/`to_string_pretty`.
- [wasmtime::component — `bindgen!` + typed records](https://docs.rs/wasmtime/latest/wasmtime/component/index.html) — generated typed bindings feed the `From` conversion.

## Related Specs

- None yet; feeds R6.3 + AC2 (IR contract) in `design.md`.
