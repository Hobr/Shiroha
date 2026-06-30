# Quality Guidelines

> Code quality standards and verification gates for backend development.

---

## Overview

This project enforces quality through automated gates that run before every commit. All code must pass formatting, linting, testing, and security checks.

Quality gates are layered:
1. **Formatting** — Code style consistency (`cargo fmt` + pre-commit hooks)
2. **Linting** — Code quality and best practices (`cargo clippy`)
3. **Testing** — Functional correctness (`cargo nextest`)
4. **Security** — Dependency audit (`cargo deny`)

---

## Quality Gate Verification

### Full Quality Check Command

```bash
just check && just test && just fmt && cargo deny check
```

**What each command does**:
- `just check` — `cargo check --workspace` (compilation)
- `just test` — `cargo nextest run --all-features --run-ignored all`
- `just fmt` — `cargo fmt --all --check` + 11 pre-commit hooks
- `cargo deny check` — License, security advisories, dependency duplicates

### Expected Outcomes

| Gate | Tool | Pass Criteria | Typical Runtime |
|------|------|---------------|-----------------|
| Compilation | `cargo check` | No errors | 5-15s |
| Formatting | `cargo fmt --check` | No diffs | <1s |
| Pre-commit | `pre-commit run --all-files` | 11 hooks pass | 2-5s |
| Linting | `cargo clippy` | No warnings | 10-20s |
| Tests | `cargo nextest run` | All tests pass | <1s (unit tests) |
| Security | `cargo deny check` | No advisories/violations | 1-3s |

### Pre-commit Hooks (11 checks)

When running `just fmt`, these hooks execute automatically:

1. **Trailing whitespace** — Remove trailing spaces
2. **End-of-file fixer** — Ensure files end with newline
3. **YAML check** — Validate YAML syntax
4. **TOML check** — Validate TOML syntax (Cargo.toml, deny.toml)
5. **Large files** — Prevent commits >500KB
6. **Merge conflicts** — Detect unresolved conflicts
7. **Debug statements** — Detect debug prints (language-specific)
8. **Private keys** — Prevent committing secrets
9. **Mixed line endings** — Enforce LF (not CRLF)
10. **Rustfmt** — Rust code formatting
11. **Clippy** — Rust linting

All hooks must pass; failures block the commit.

---

## Forbidden Patterns

### Type Duplication Across Crates

**Don't**: Redefine contract types in downstream crates.

```rust
// ❌ crates/wasm/src/types.rs
pub struct ActionContext {
    pub task_id: String,
    pub event: Option<String>,
}
```

**Do**: Import from the canonical source.

```rust
// ✅ crates/wasm/src/adapter.rs
use shiroha_engine::ActionContext;
```

**Why**: Single source of truth prevents divergence. See [WASM Component Integration](./wasm-component-integration.md#single-source-contract-principle).

### Circular Dependencies

**Don't**: Create circular crate dependencies.

```toml
# ❌ Creates cycle
# crates/ir/Cargo.toml
[dependencies]
shiroha-engine = { path = "../engine" }

# crates/engine/Cargo.toml
[dependencies]
shiroha-ir = { path = "../ir" }
```

**Do**: Follow dependency layers (ir ← engine ← wasm).

**Why**: Circular dependencies prevent compilation and violate layered architecture. See [Rust Workspace Structure](./rust-workspace-structure.md#dependency-direction).

### Ignoring Clippy Warnings

**Don't**: Commit code with clippy warnings.

```rust
// ❌ Clippy warning: collapsible_if
if condition_a {
    if condition_b {
        do_something();
    }
}
```

**Do**: Fix warnings before commit.

```rust
// ✅ Collapsed
if condition_a && condition_b {
    do_something();
}
```

**Why**: Clippy warnings often indicate real issues or unidiomatic code.

---

## Required Patterns

### Workspace Dependency Inheritance

**Always** use `workspace = true` for shared dependencies.

```toml
# ✅ Workspace root
[workspace.dependencies]
tokio = { version = "1.52", features = ["full"] }

# ✅ Member crate
[dependencies]
tokio.workspace = true
```

**Why**: Prevents version conflicts, ensures consistent features. See [Rust Workspace Structure](./rust-workspace-structure.md#workspace-dependency-inheritance).

### Error Propagation

**Library crates**: Use `thiserror` for typed errors.
**Application crates**: Use `anyhow` for error context.

```rust
// ✅ Library error (shiroha-ir)
use thiserror::Error;

#[derive(Error, Debug)]
pub enum IrError {
    #[error("Duplicate state ID: {0}")]
    DuplicateStateId(String),
}

// ✅ Application error (shirohad)
use anyhow::{Context, Result};

fn load_config() -> Result<Config> {
    let path = "config.toml";
    std::fs::read_to_string(path)
        .context("Failed to read config file")?;
    // ...
}
```

**Why**: Libraries need typed errors for callers; applications need rich context for debugging.

### Serialization Support for IR Types

**Always** derive `Serialize` and `Deserialize` for IR types that cross boundaries.

```rust
// ✅ IR type with serialization
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachineDef {
    pub name: String,
    pub initial: StateId,
    pub states: Vec<State>,
}
```

**Why**: IR types must serialize for persistence, WASM boundaries, and network transport.

---

## Testing Requirements

### Unit Tests (Per Crate)

Each crate must have unit tests covering:
- Happy path (valid inputs → expected outputs)
- Error cases (invalid inputs → expected errors)
- Edge cases (boundary conditions)

**Location**: `src/tests.rs` with `#[cfg(test)]` or `tests/*.rs` files.

**Example**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_state_machine() {
        let def = StateMachineDef {
            name: "test".into(),
            initial: StateId(0),
            states: vec![/* ... */],
        };
        assert!(def.validate().is_ok());
    }

    #[test]
    fn test_duplicate_state_id() {
        let def = /* invalid def with duplicate */;
        assert!(matches!(
            def.validate(),
            Err(IrError::DuplicateStateId(_))
        ));
    }
}
```

**Run**: `cargo nextest run -p <crate-name>`

### Integration Tests

For cross-crate workflows (e.g., WASM adapter → IR → engine).

**Location**: `crates/<crate>/tests/*.rs` or top-level `tests/` directory.

**Example**:
```rust
// tests/wasm_integration_test.rs
#[tokio::test]
async fn test_wasm_component_load() {
    let adapter = WasmAdapter::from_file("examples/sm-example/target/...").unwrap();
    let ir = adapter.load().await.unwrap();
    assert_eq!(ir.name, "example-sm");
}
```

**Run**: `cargo nextest run --workspace`

### Coverage Goals

- **Critical paths**: 80%+ coverage (state machine runtime, IR validation)
- **Happy paths**: All tested
- **Error paths**: All error variants tested

**Check coverage**:
```bash
just coverage
# or
cargo llvm-cov nextest --html
```

---

## Dependency Audit Expectations

### cargo deny check

**Expected warnings**: Wasmtime ecosystem produces duplicate dependency warnings (non-blocking).

```
warning: 8 duplicate dependencies:
  cpufeatures v0.2.16 (2 versions)
  getrandom v0.2.15 (2 versions)
  hashbrown v0.15.2 (2 versions)
  ...
```

**Why non-blocking**: These duplicates come from wasmtime's transitive dependencies (cranelift, wasm-encoder, wiggle) using different versions internally. Not a project configuration issue.

**Blocking errors**:
- Security advisories (must fix or explicitly accept)
- License violations (must resolve)
- Banned dependencies (must remove)

### Updating Dependencies

Follow [Cargo Conventions](./cargo-conventions.md):
1. Update `[workspace.dependencies]` in root `Cargo.toml`
2. Run `cargo update` to refresh `Cargo.lock`
3. Run full quality gate (`just check && just test`)
4. Commit both `Cargo.toml` and `Cargo.lock`

---

## Code Review Checklist

Before merging:

- [ ] **Compilation**: `cargo check --workspace` passes
- [ ] **Formatting**: `cargo fmt --all --check` passes (or run `just fmt` to auto-fix)
- [ ] **Linting**: `cargo clippy --workspace` has no warnings
- [ ] **Tests**: `cargo nextest run --workspace` all pass
- [ ] **Security**: `cargo deny check` has no blocking errors
- [ ] **No type duplication**: Check new types aren't redefined across crates
- [ ] **No circular deps**: Verify dependency direction follows layers (ir ← engine ← wasm)
- [ ] **Tests for new code**: New functions/modules have unit tests
- [ ] **Error handling**: Library errors use `thiserror`, app errors use `anyhow`
- [ ] **Commit message**: Follows [Git Commit Conventions](./git-commit-conventions.md)

---

## Common Quality Issues

### Issue: Tests Pass Locally But Fail in CI

**Symptom**: `cargo test` succeeds, but CI fails with timing or ordering issues.

**Cause**: Tests depend on execution order or timing.

**Fix**: Use `cargo nextest` locally (same tool as CI):
```bash
cargo nextest run --all-features --run-ignored all
```

### Issue: Clippy Passes But Code Is Hard to Read

**Symptom**: No clippy warnings, but reviewers struggle to understand code.

**Fix**: Clippy catches mechanical issues, not design. Consider:
- Breaking large functions into smaller ones
- Adding doc comments (`///`)
- Renaming variables for clarity
- Simplifying control flow

### Issue: cargo deny Fails After Dependency Update

**Symptom**: `cargo deny check` reports new advisories or license issues.

**Cause**: Updated dependency introduced vulnerability or changed license.

**Fix**:
1. Check advisory details: `cargo audit`
2. Update to patched version if available
3. If no patch exists, evaluate risk and consider alternatives
4. Document decision in `deny.toml` if accepting risk

---

## Related

- [Rust Workspace Structure](./rust-workspace-structure.md) — Crate organization and dependency rules
- [WASM Component Integration](./wasm-component-integration.md) — Contract testing and type validation
- [Git Commit Conventions](./git-commit-conventions.md) — When to commit after quality gates pass
