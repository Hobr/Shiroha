# Rust Workspace Structure Guidelines

> Conventions for organizing Rust workspace crates, dependencies, and module boundaries.

---

## 1. Scope / Trigger

**Trigger**: Setting up a new Rust workspace or adding crates to an existing one.

**Applies to**: Multi-crate Rust projects using Cargo workspace features.

---

## 2. Workspace Layout

### Directory Structure

```
project-root/
├── Cargo.toml              # Workspace manifest
├── crates/                 # Library crates
│   ├── <name>/
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs    # Public types
│   │       ├── error.rs    # Error types
│   │       └── tests.rs    # Unit tests (cfg(test))
├── bin/                    # Binary crates
│   └── <name>/
│       ├── Cargo.toml
│       └── src/
│           └── main.rs
└── examples/               # Example/demo crates
    └── <name>/
        ├── Cargo.toml
        └── src/
            └── lib.rs      # or main.rs for binary examples
```

### Naming Conventions

| Type | Location | Name Pattern | Example |
|------|----------|--------------|---------|
| Library | `crates/` | `<project>-<module>` | `shiroha-ir`, `shiroha-engine` |
| Binary | `bin/` | `<command-name>` | `shirohad`, `sctl` |
| Example | `examples/` | `<example-name>` | `sm-example` |

**Rationale**:
- Library crates use project prefix for namespacing (prevents name collisions on crates.io)
- Binary crates use command name directly (what users type in terminal)
- Examples use descriptive names without prefix (internal-only)

---

## 3. Workspace Manifest Contract

### Required Sections

```toml
[workspace]
members = [
    "crates/*",
    "bin/*",
    "examples/*",
]
resolver = "3"

[workspace.package]
version = "0.1.0"
edition = "2024"
rust-version = "1.95.0"
license = "GPL-3.0-only"
repository = "https://github.com/org/repo"

[workspace.dependencies]
# Group by category with comments
# Async runtime
tokio = { version = "1.52", features = ["full"] }
async-trait = "0.1"

# Error handling
thiserror = "2.0"
anyhow = "1.0"
```

### Dependency Categories

Group `[workspace.dependencies]` with comment headers:

1. **Async runtime** — tokio, async-trait, futures
2. **WASM runtime** — wasmtime, wasmtime-wasi
3. **CLI** — clap, clap_complete
4. **Network** — tonic, prost, reqwest
5. **Config** — config, serde
6. **Error handling** — thiserror, anyhow
7. **Build** — shadow-rs

**Rationale**: Grouped dependencies improve readability and prevent duplicate entries.

---

## 4. Crate Manifest Contract

### Member Crate Pattern

```toml
[package]
name = "shiroha-ir"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
# External dependencies
thiserror.workspace = true
serde = { workspace = true, features = ["derive"] }

# Internal dependencies (use path)
# shiroha-types = { path = "../types" }

[dev-dependencies]
# Test-only dependencies
```

**Rules**:
- Inherit `version`, `edition`, `rust-version`, `license`, `repository` from workspace
- Use `workspace = true` for external dependencies
- Use `path = "../<crate>"` for internal dependencies
- Keep `[dependencies]` sorted alphabetically

---

## 5. Module Boundary Rules

### Dependency Direction

```
┌─────────────────────────────────────┐
│  bin/shirohad, bin/sctl (binaries) │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│  crates/control, crates/plugin      │  ← Integration layer
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│  crates/engine, crates/wasm         │  ← Execution layer
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│  crates/ir (IR types only)          │  ← Data layer (no dependencies)
└─────────────────────────────────────┘
```

**Rules**:
1. **Data layer** (`ir`) has zero internal dependencies
2. **Execution layer** depends only on data layer
3. **Integration layer** depends on execution + data
4. **Binaries** depend on integration + execution + data

**Forbidden**:
- ❌ Circular dependencies (A depends on B, B depends on A)
- ❌ IR depending on runtime crates (breaks clean separation)
- ❌ Execution layer depending on integration layer (inverses ownership)

---

## 6. Validation & Error Matrix

| Condition | Error Message | Fix |
|-----------|---------------|-----|
| Circular dependency | `cyclic package dependency` | Refactor to extract shared types into a lower-layer crate |
| Missing workspace inheritance | `version not specified` | Add `version.workspace = true` |
| Duplicate dependency versions | `multiple versions of X` | Use `workspace = true` for all occurrences |
| Wrong crate location | (organizational) | Move library crates to `crates/`, binaries to `bin/` |

---

## 7. Good/Base/Bad Cases

### Good: Clean Layer Separation

```toml
# crates/ir/Cargo.toml (data layer)
[dependencies]
serde.workspace = true
thiserror.workspace = true
# No internal dependencies

# crates/engine/Cargo.toml (execution layer)
[dependencies]
shiroha-ir = { path = "../ir" }
tokio.workspace = true
async-trait.workspace = true
# Depends only on lower layer (ir)
```

### Base: Direct Binary Implementation

```toml
# bin/shirohad/Cargo.toml
[dependencies]
shiroha-control = { path = "../../crates/control" }
shiroha-engine = { path = "../../crates/engine" }
tokio.workspace = true
clap.workspace = true
# Binary can depend on multiple crates
```

### Bad: Circular Dependencies

```toml
# ❌ crates/ir/Cargo.toml
[dependencies]
shiroha-engine = { path = "../engine" }  # IR should NOT depend on engine

# ❌ crates/engine/Cargo.toml
[dependencies]
shiroha-ir = { path = "../ir" }
# Creates cycle: ir ← engine ← ir
```

---

## 8. Tests Required

### Unit Tests (Per Crate)

```bash
cargo nextest run -p shiroha-ir
cargo nextest run -p shiroha-engine
```

**Assertion points**:
- Each crate compiles independently
- Tests run in isolation (no cross-crate test dependencies)

### Workspace Tests

```bash
cargo nextest run --workspace
```

**Assertion points**:
- All crates compile together
- No version conflicts
- No circular dependencies

### Build Verification

```bash
cargo check --workspace
cargo build --workspace --all-features
```

**Assertion points**:
- No compilation errors
- Feature flags don't introduce conflicts

---

## 9. Wrong vs Correct

### Wrong: Flat Structure

```
❌ Bad:
project-root/
├── Cargo.toml
├── ir/              # Not in crates/
├── engine/          # Not in crates/
└── shirohad/        # Binary mixed with libraries
```

**Problem**: No clear separation between libraries and binaries; harder to understand project structure.

### Correct: Layered Structure

```
✅ Good:
project-root/
├── Cargo.toml       # Workspace root
├── crates/          # All libraries here
│   ├── ir/
│   └── engine/
├── bin/             # All binaries here
│   └── shirohad/
└── examples/        # All examples here
    └── sm-example/
```

**Benefit**: Clear separation; easy to navigate; follows Rust community conventions.

---

## 10. Design Decisions

### Decision: `resolver = "3"`

**Context**: Cargo has two dependency resolvers (v2 vs v3).

**Decision**: Always use `resolver = "3"` (Rust 2024 default).

**Why**:
- Better feature unification across workspace
- Matches Rust 2024 edition
- Avoids dependency version conflicts

### Decision: Workspace Dependency Inheritance

**Context**: Dependencies can be defined per-crate or in workspace.

**Decision**: Define all shared dependencies in `[workspace.dependencies]`, reference with `workspace = true`.

**Why**:
- Single source of truth for versions
- Prevents version skew across crates
- Easier to upgrade dependencies (one place)

**Example**:
```toml
# Workspace root
[workspace.dependencies]
tokio = { version = "1.52", features = ["full"] }

# Member crate
[dependencies]
tokio.workspace = true  # Inherits version and features
```

### Decision: Library vs Binary Naming

**Context**: Should binary crates have the project prefix?

**Decision**:
- Libraries: `<project>-<module>` (e.g., `shiroha-engine`)
- Binaries: `<command>` (e.g., `shirohad`, not `shiroha-daemon`)

**Why**:
- Library names must be unique for crates.io
- Binary names should match what users type
- Avoids confusion between `use shiroha_engine` (library) and `shirohad` (command)

---

## 11. Common Mistakes

### Mistake: Not Using Workspace Dependencies

**Symptom**: Different crates use different versions of the same dependency.

```toml
# ❌ crates/ir/Cargo.toml
tokio = "1.52"

# ❌ crates/engine/Cargo.toml
tokio = "1.51"  # Different version!
```

**Fix**: Define once in workspace, reference everywhere.

```toml
# ✅ Workspace root
[workspace.dependencies]
tokio = { version = "1.52", features = ["full"] }

# ✅ Member crates
[dependencies]
tokio.workspace = true
```

### Mistake: Circular Internal Dependencies

**Symptom**: `cargo build` fails with `cyclic package dependency`.

**Cause**: Two crates depend on each other (A → B → A).

**Fix**: Extract shared types into a lower-layer crate:

```
Before (circular):
ir ←→ engine

After (layered):
ir-types ← ir ← engine
```

### Mistake: Missing resolver = "3"

**Symptom**: Feature flags behave inconsistently across crates.

**Fix**: Add to workspace manifest:

```toml
[workspace]
resolver = "3"
```

---

## 12. Extension Point Placeholder Pattern

### Pattern: Reserve Fields for Future Features Without Breaking Changes

When designing types that will be serialized or part of public API, reserve fields for planned-but-not-yet-implemented features to avoid breaking changes in future versions.

**Example**: Orthogonal regions placeholder in IR types

```rust
// File: crates/ir/src/types.rs

/// Placeholder for future orthogonal region support.
/// TODO: Define orthogonal region structure for future use.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct OrthogonalRegion {
    // Empty for MVP; to be filled in post-MVP when implementing
    // hierarchical concurrent regions (UML orthogonal states).
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub id: StateId,
    pub parent: Option<StateId>,
    pub entry: Option<ActionRef>,
    pub exit: Option<ActionRef>,
    pub do_activity: Option<ActionRef>,
    pub history: HistoryConfig,

    /// Reserved for future hierarchical concurrent regions.
    /// Always None in MVP; will be populated post-MVP.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ortho: Option<OrthogonalRegion>,
}
```

**Why this works**:
- Field exists in type signature but is always `None` during MVP.
- Future implementation fills the struct without changing `State` public API.
- `#[serde(skip_serializing_if = "Option::is_none")]` keeps serialized format clean.
- `#[allow(dead_code)]` on the placeholder struct silences unused warnings.
- No breaking change when the feature is implemented later.

**When to use**:
- Feature is in design spec but deferred to later version (documented roadmap).
- Adding field later would break serialization compatibility or public API.
- Type is part of cross-crate contract or external API surface.
- The placeholder documents future intent clearly.

**When NOT to use**:
- Feature is purely speculative (not in design or roadmap).
- Field can be added later without breaking changes (e.g., internal private struct).
- The type is purely internal implementation detail with no stability guarantee.

**Comparison to other approaches**:

| Approach | Pros | Cons |
|----------|------|------|
| **Placeholder field (recommended)** | No breaking change later; documents intent; serialization-safe | Takes up small space even when unused |
| **Add field later** | Cleaner initial code | Breaking change; requires migration; version bump |
| **Feature flag** | Can be disabled | Complexity; still needs placeholder or breaking change |

---

## 13. Extensibility

### Adding a New Crate

1. Create directory in appropriate location (`crates/`, `bin/`, or `examples/`)
2. Initialize with `cargo init --lib` or `cargo init --bin`
3. Update workspace `members = [...]` if not using glob pattern
4. Inherit workspace metadata in crate's `Cargo.toml`
5. Add internal dependencies using `path = "../<crate>"`

### Splitting a Large Crate

When a crate grows too large (>2000 LoC):

1. Identify a cohesive module (e.g., types, validation, execution)
2. Extract to new crate following layer rules
3. Update dependent crates to reference the new crate
4. Verify no circular dependencies introduced

**Example**: If `shiroha-engine` grows to 5000 LoC, extract:
- `shiroha-engine-types` (data types)
- `shiroha-engine-runtime` (execution logic)

---

## Related

- [Cargo Conventions](./cargo-conventions.md) — Dependency management, initialization
- [Git Commit Conventions](./git-commit-conventions.md) — Commit cadence for crate changes
- [WASM Component Integration](./wasm-component-integration.md) — WIT contracts and type mapping patterns
- [Code Reuse Thinking Guide](../guides/code-reuse-thinking-guide.md) — Avoid duplicating types across crates
