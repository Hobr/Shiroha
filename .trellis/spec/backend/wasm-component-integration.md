# WASM Component Model Integration

> Contract-driven WASM Component Model integration pattern for state machine runtime.

---

## 1. Scope / Trigger

**Trigger**: Integrating WebAssembly Component Model components into the Shiroha runtime, defining WIT interfaces, or implementing host/guest bindings.

**Applies to**: 
- Defining new WIT interfaces for framework capabilities
- Implementing WASM adapters or invokers
- Adding host-imported capabilities to guest components
- Mapping between WIT types and Rust types

---

## 2. Signatures

### WIT Interface (Contract Definition)

**File**: `wit/state-machine.wit`

```wit
package shiroha:sm@0.1.0;

interface types {
    variant action-kind {
        wasm(string),
        plugin(string),
    }
    
    record action-ref {
        name: string,
        kind: action-kind,
    }
    
    record state {
        name: string,
        parent: option<string>,
        entry: option<action-ref>,
        exit: option<action-ref>,
        do-activity: option<action-ref>,
        history: history-kind,
    }
}

interface actions {
    use action-types.{action-context, action-result};
    
    /// Invoke a synchronous action.
    invoke: func(ctx: action-context) -> result<action-result, string>;
    
    /// Invoke an async do-activity.
    invoke-do: func(ctx: action-context) -> result<action-result, string>;
}

world state-machine {
    export definition;
    export actions;
    import host;
}
```

### Rust Type Mapping (Host-side)

**File**: `crates/engine/src/action.rs`

```rust
use serde::{Deserialize, Serialize};

/// Context passed to action invocations.
/// Mirrors WIT action-context record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionContext {
    pub task_id: String,
    pub event: Option<String>,
    pub payload: Option<Vec<u8>>,
}

/// Result of an action invocation.
/// Mirrors WIT action-result variant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionResult {
    Ok,
    OkValue(Vec<u8>),
    Error(String),
    Signal(String),
}
```

### Adapter Trait (Engine Contract)

**File**: `crates/engine/src/adapter.rs`

```rust
use async_trait::async_trait;
use shiroha_ir::StateMachineDef;

/// Adapter trait for loading state machine definitions.
/// WASM, file, and plugin adapters all implement this.
#[async_trait]
pub trait Adapter: Send + Sync {
    async fn load(&self) -> anyhow::Result<StateMachineDef>;
}
```

---

## 3. Contracts

### WIT ↔ Rust Type Mapping Contract

| WIT Type | Rust Type | Crate | Notes |
|----------|-----------|-------|-------|
| `action-context` | `ActionContext` | `shiroha-engine` | Single definition, serializable |
| `action-result` | `ActionResult` | `shiroha-engine` | Single definition, serializable |
| `state` | `State` | `shiroha-ir` | IR representation, differs from WIT structure |
| `transition` | `Transition` | `shiroha-ir` | IR representation |
| `action-ref` | `ActionRef` | `shiroha-ir` | Kind enum maps to WIT variant |

### Single-Source Contract Principle

1. **WIT is the authoritative contract**: `wit/*.wit` files define the Component Model boundary.
2. **Rust types defined once**: Each contract type has ONE canonical definition in its owning crate.
3. **WASM crate consumes, never redefines**: `shiroha-wasm` imports types from `shiroha-engine` and `shiroha-ir`, implements traits, but NEVER duplicates type definitions.

### Dependency Flow

```
WIT definition (contract specification)
    ↓
shiroha-ir (IR types, no runtime dependencies)
    ↓
shiroha-engine (runtime types, adapter/invoker traits)
    ↓
shiroha-wasm (Component Model bindings, trait implementations)
```

**Critical Rule**: No circular dependencies. WASM crate depends on engine and IR; engine and IR NEVER depend on wasmtime or WASM-specific code.

---

## 4. Validation & Error Matrix

### Build-time Validation

| Condition | Tool | Expected Outcome |
|-----------|------|------------------|
| WIT syntax valid | `wit-bindgen` / wasmtime | Binding generation succeeds |
| Guest component builds | `cargo build --target wasm32-wasip2` | Component binary produced |
| Host crate compiles | `cargo check -p shiroha-wasm` | No compilation errors |
| Type mapping consistent | Manual review | Rust types mirror WIT structure |

### Runtime Validation

| Condition | Error Type | Handling |
|-----------|------------|----------|
| Component load fails | `WasmError::ComponentLoad` | Return error to caller, log details |
| Instantiation fails | `WasmError::Instantiation` | Check host imports match WIT |
| Function call fails | `WasmError::Invocation` | Propagate to action result |
| Type mismatch at boundary | `WasmError::TypeMismatch` | Should be prevented by bindgen; fatal if occurs |

---

## 5. Good/Base/Bad Cases

### Good: Clean WIT Contract Definition

```wit
// Good: Clear separation of concerns
interface types {
    record state {
        name: string,
        parent: option<string>,
    }
}

interface actions {
    use types.{action-context, action-result};
    invoke: func(ctx: action-context) -> result<action-result, string>;
}

world state-machine {
    export actions;
    import host;
}
```

**Why**: Types in one interface, actions in another. World clearly specifies exports and imports.

### Base: Rust Type Mirrors WIT

```rust
// Base: Direct structural mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionContext {
    pub task_id: String,
    pub event: Option<String>,
    pub payload: Option<Vec<u8>>,
}
```

**Why**: Field names and types match WIT exactly. Serialization support for boundary crossing.

### Bad: Duplicate Type Definitions

```rust
// Bad: Type redefined in WASM crate
// File: crates/wasm/src/types.rs
pub struct ActionContext {  // ❌ Duplicate!
    pub task_id: String,
    pub event: Option<String>,
}

// File: crates/wasm/src/adapter.rs
impl Adapter for WasmAdapter {
    async fn load(&self) -> Result<StateMachineDef> {
        let ctx = ActionContext { ... };  // ❌ Using local type
    }
}
```

**Why it's bad**: Breaks single-source principle. Causes type divergence. Creates coupling issues.

**Correct approach**:
```rust
// Good: Import from engine
use shiroha_engine::{ActionContext, ActionResult};

impl Adapter for WasmAdapter {
    async fn load(&self) -> Result<StateMachineDef> {
        let ctx = ActionContext { ... };  // ✅ Using canonical type
    }
}
```

---

## 6. Tests Required

### Unit Tests (WASM Crate)

**File**: `crates/wasm/src/tests.rs` or `crates/wasm/tests/*.rs`

```rust
#[test]
fn test_wasm_adapter_from_bytes() {
    // Given: Valid WASM component bytes
    let component_bytes = include_bytes!("../../fixtures/simple.wasm");
    
    // When: Create adapter from bytes
    let adapter = WasmAdapter::from_bytes(component_bytes);
    
    // Then: Adapter created successfully
    assert!(adapter.is_ok());
}

#[tokio::test]
async fn test_wasm_adapter_load_ir() {
    // Given: Adapter with valid component
    let adapter = WasmAdapter::from_file("examples/sm-example/target/wasm32-wasip2/release/shiroha_sm_example.wasm").unwrap();
    
    // When: Load state machine definition
    let ir = adapter.load().await;
    
    // Then: IR structure matches component definition
    assert!(ir.is_ok());
    let def = ir.unwrap();
    assert_eq!(def.name, "example-sm");
    assert!(!def.states.is_empty());
}
```

**Assertion Points**:
- Component loads without errors
- IR structure extracted correctly from WIT definition exports
- Action references map to correct kinds (wasm/plugin)
- Host imports linked successfully

### Integration Tests

**File**: `crates/wasm/tests/integration_test.rs`

```rust
#[tokio::test]
async fn test_end_to_end_wasm_execution() {
    // Given: WASM component loaded as adapter
    let adapter = WasmAdapter::from_file("...").unwrap();
    let ir = adapter.load().await.unwrap();
    
    // And: Action invoker for the component
    let invoker = WasmActionInvoker::new("...").unwrap();
    
    // When: Invoke synchronous action
    let ctx = ActionContext {
        task_id: "test-task".into(),
        event: Some("entry".into()),
        payload: None,
    };
    let result = invoker.invoke_sync("on_entry", ctx).await;
    
    // Then: Action executed and result returned
    assert!(matches!(result, Ok(ActionResult::Ok)));
}
```

**Assertion Points**:
- Full load → instantiate → invoke chain works
- Action context serializes across boundary
- Action result deserializes correctly
- Host imports (log) callable from guest

---

## 7. Wrong vs Correct

### Wrong: Redefining WIT-Mapped Types

#### Wrong

```rust
// File: crates/wasm/src/action.rs
// ❌ Duplicate definition
pub struct ActionContext {
    pub task_id: String,
    pub event: Option<String>,
    pub payload: Option<Vec<u8>>,
}

// File: crates/wasm/src/invoker.rs
use crate::action::ActionContext;  // ❌ Using local type

impl ActionInvoker for WasmActionInvoker {
    async fn invoke_sync(&self, name: &str, ctx: ActionContext) -> Result<ActionResult> {
        // ...
    }
}
```

**Problems**:
1. Type divergence: If `shiroha-engine::ActionContext` changes, WASM crate breaks silently.
2. Trait implementation fails: `ActionInvoker` trait expects `shiroha_engine::ActionContext`, not local type.
3. Violates single-source principle.

#### Correct

```rust
// File: crates/wasm/src/invoker.rs
use shiroha_engine::{ActionInvoker, ActionContext, ActionResult};  // ✅ Import from engine

pub struct WasmActionInvoker {
    // ...
}

#[async_trait]
impl ActionInvoker for WasmActionInvoker {
    async fn invoke_sync(&self, name: &str, ctx: ActionContext) -> anyhow::Result<ActionResult> {
        // Serialize ctx to WIT boundary
        // Call WASM function
        // Deserialize result from WIT boundary
        Ok(ActionResult::Ok)
    }
}
```

**Why correct**:
- Uses canonical types from `shiroha-engine`
- Trait implementation type-checks automatically
- Changes to contract propagate correctly
- WASM crate is pure consumer

---

### Wrong: Circular Dependencies

#### Wrong

```toml
# File: crates/engine/Cargo.toml
[dependencies]
shiroha-wasm = { path = "../wasm" }  # ❌ Engine depends on WASM

# File: crates/wasm/Cargo.toml
[dependencies]
shiroha-engine = { path = "../engine" }  # ❌ Circular!
```

**Problems**:
1. Circular dependency prevents compilation.
2. Engine becomes coupled to WASM runtime (wasmtime).
3. Violates layered architecture.

#### Correct

```toml
# File: crates/engine/Cargo.toml
[dependencies]
shiroha-ir = { path = "../ir" }
# NO dependency on shiroha-wasm

# File: crates/wasm/Cargo.toml
[dependencies]
shiroha-engine = { path = "../engine" }  # ✅ One-way dependency
shiroha-ir = { path = "../ir" }
wasmtime = { workspace = true }
```

**Why correct**:
- Dependency flows one direction: wasm → engine → ir
- Engine defines traits, WASM implements them
- Engine remains runtime-agnostic
- wasmtime isolated in WASM crate

---

### Wrong: Ignoring WIT Contract Changes

#### Wrong

```wit
<!-- File: wit/state-machine.wit -->
<!-- WIT contract updated -->
record action-context {
    task-id: string,
    event: option<string>,
    payload: option<list<u8>>,
    timestamp: u64,  // ← New field added
}
```

```rust
// File: crates/engine/src/action.rs
// ❌ Rust type not updated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionContext {
    pub task_id: String,
    pub event: Option<String>,
    pub payload: Option<Vec<u8>>,
    // Missing: timestamp field
}
```

**Problems**:
1. Type mismatch at WIT boundary.
2. Binding generation may fail or produce incorrect code.
3. Runtime errors when guest sends new field.

#### Correct

```rust
// File: crates/engine/src/action.rs
// ✅ Rust type updated to match WIT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionContext {
    pub task_id: String,
    pub event: Option<String>,
    pub payload: Option<Vec<u8>>,
    pub timestamp: u64,  // ✅ Added
}
```

**Process**:
1. Update WIT contract first (source of truth).
2. Update Rust type in `shiroha-engine` to mirror WIT.
3. Regenerate bindings (if using `wit-bindgen`).
4. Update all usage sites (compiler will catch them).
5. Run integration tests to verify boundary crossing.

---

## 8. Extension Point Placeholder Pattern

### Pattern: Reserve Fields for Future Features

When designing IR or WIT types, reserve fields for planned-but-not-yet-implemented features to avoid breaking changes later.

**Example**: Orthogonal regions placeholder in IR

```rust
// File: crates/ir/src/types.rs

/// Placeholder for future orthogonal region support.
/// TODO: Define orthogonal region structure for future use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrthogonalRegion {
    // Empty for MVP; to be filled in post-MVP
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub id: StateId,
    pub parent: Option<StateId>,
    pub entry: Option<ActionRef>,
    pub exit: Option<ActionRef>,
    pub do_activity: Option<ActionRef>,
    pub history: HistoryConfig,
    
    // Reserved for future hierarchical concurrent regions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ortho: Option<OrthogonalRegion>,
}
```

**Why this works**:
- `ortho` field exists in type definition but is always `None` in MVP.
- Future implementation fills `OrthogonalRegion` struct without changing `State` signature.
- Serialization skips `None` values, keeping serialized format clean.
- `#[allow(dead_code)]` on `OrthogonalRegion` silences compiler warnings.

**When to use**:
- Feature is in design spec but deferred to later version.
- Adding field later would break serialization compatibility.
- Type is part of public API or cross-crate contract.

**When NOT to use**:
- Feature is speculative (not in design).
- Field can be added later without breaking changes (e.g., internal implementation detail).

---

## 9. Async Strategy Evolution

### MVP Strategy (v0.2.0): Synchronous Wrapper

**Context**: Component Model async support (future types) in wasmtime 46.x requires verification and may not be stable for MVP.

**Design Intent** (from `design.md`):
```wit
interface actions {
    /// Invoke an async do-activity (returns a future).
    invoke-do: func(ctx: action-context) -> future<result<action-result, string>>;
}
```

**MVP Implementation** (v0.2.0):
```wit
interface actions {
    /// Invoke an async do-activity.
    /// Note: Component Model async (future) support requires wasmtime 46.x+ verification.
    /// MVP implementation will use synchronous wrapper with tokio cancellable task.
    invoke-do: func(ctx: action-context) -> result<action-result, string>;
}
```

**Rationale**:
- Wasmtime 46.x Component Model async maturity unverified.
- Fallback: Synchronous signature + tokio task wrapping for cancellation.
- WIT comment documents evolution path.

### Post-MVP Path

Once Component Model async is verified stable:

1. Update WIT to use `future<T>` return type.
2. Update Rust trait to return `impl Future` or async fn.
3. Update WASM invoker to await future from guest.
4. Update all guest components to export async functions.

**Migration strategy**:
- WIT version bump (`shiroha:sm@0.2.0`).
- Compatibility shim: Old synchronous components wrapped in futures.
- Gradual migration: New components use async, old ones continue working.

---

## 10. Build Configuration

### WASM Crate Build Script

**File**: `crates/wasm/build.rs`

```rust
fn main() {
    // Watch WIT directory for changes
    println!("cargo:rerun-if-changed=../../wit");
    
    // Note: No bindgen generation in build script.
    // We use inline wasmtime::component::bindgen! in source
    // once wasmtime 46.x macro requirements are clarified.
}
```

**Purpose**:
- Trigger rebuild when WIT contracts change.
- Defers binding generation to source code (inline `bindgen!` macro).
- Avoids build-time complexity until wasmtime macro is stable.

### Example Component Build

**File**: `examples/sm-example/build.rs`

```rust
fn main() {
    // Watch WIT directory (guest uses same contracts)
    println!("cargo:rerun-if-changed=../../wit");
    
    // Guest-side bindgen will be added here post-MVP
}
```

**Build command**:
```bash
cargo build -p shiroha-sm-example --target wasm32-wasip2
```

---

## Common Mistakes

### Mistake 1: Coupling Engine to WASM Runtime

**Symptom**: `shiroha-engine` imports `wasmtime` or WASM-specific types.

**Fix**: Keep engine runtime-agnostic. WASM adapter implements engine traits but engine doesn't know about WASM.

### Mistake 2: Forgetting to Update Both WIT and Rust

**Symptom**: Type mismatch errors at runtime after changing contract.

**Fix**: WIT is source of truth. Update WIT first, then mirror changes in Rust types.

### Mistake 3: Testing Only Happy Path

**Symptom**: Integration tests don't catch component load failures or invocation errors.

**Fix**: Test error conditions:
- Invalid component bytes
- Missing host imports
- Type mismatches
- Action invocation failures

### Mistake 4: Not Watching WIT in Build Scripts

**Symptom**: Changes to WIT don't trigger recompilation; stale bindings used.

**Fix**: Add `println!("cargo:rerun-if-changed=../../wit");` to `build.rs`.

---

## Decision Log

### Decision: WIT as Single Source of Truth

**Date**: 2026-06-30 (v0.2.0)

**Context**: Need authoritative contract between host and guest components.

**Decision**: WIT interface definitions are the single source of truth for Component Model contracts. Rust types mirror WIT structure but do not define it.

**Consequences**:
- WIT changes drive Rust type changes.
- Binding generation owns type mapping.
- Version compatibility tracked via WIT package versions.

### Decision: Single-Point Type Definitions

**Date**: 2026-06-30 (v0.2.0)

**Context**: Multiple crates need to share contract types (ActionContext, ActionResult).

**Decision**: Each contract type has ONE canonical definition in its owning crate. Downstream crates import, never redefine.

**Mapping**:
- WIT boundary types → `shiroha-engine` (ActionContext, ActionResult)
- IR types → `shiroha-ir` (StateMachineDef, State, Transition)
- WASM crate → pure consumer, implements traits

**Consequences**:
- No type divergence.
- Changes propagate automatically via compiler.
- Clear ownership model.

### Decision: Defer Component Model Async to Post-MVP

**Date**: 2026-06-30 (v0.2.0)

**Context**: Wasmtime 46.x Component Model async (future types) maturity unverified.

**Decision**: v0.2.0 MVP uses synchronous `invoke-do` signature with tokio cancellable task wrapper. Post-MVP will migrate to proper Component Model futures once stable.

**Consequences**:
- MVP can ship without async risk.
- WIT comment documents evolution path.
- Future migration requires WIT version bump.

---

## Related Specs

- [Rust Workspace Structure](./rust-workspace-structure.md) — Crate organization and dependency flow
- [HSM Implementation Pattern](./hsm-implementation-pattern.md) — State machine runtime integration points
