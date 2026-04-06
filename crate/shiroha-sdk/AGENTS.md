<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-06 | Updated: 2026-04-06 -->

# shiroha-sdk

## Purpose

Rust guest component 开发 SDK。为 Flow / Network Flow / Storage Flow / Full Flow 提供生成宏，并封装常用 helper，让开发者不必在每个 guest crate 里直接手写 `wit_bindgen::generate!`。

## Key Files

| File | Description |
| ---- | ----------- |
| `src/lib.rs` | `generate_flow!` / `generate_network_flow!` / `generate_storage_flow!` / `generate_full_flow!` 宏，以及常用 helper 宏 |
| `Cargo.toml` | SDK crate 定义，当前依赖 `wit-bindgen` |

## For AI Agents

### Working In This Directory

- 这是 guest-facing API，优先关注“接入成本”和“升级兼容性”
- 宏展开使用的 canonical WIT 定义位于 `crate/shiroha-wit/wit/`
- 这里的 helper 宏应尽量只依赖 guest 作用域里已经生成的类型名，避免引入额外样板
- 如果新增 capability world，优先先补 SDK 生成宏，再补 examples

### Testing Requirements

- `cargo check -p shiroha-sdk`
- 至少编译一个 `example/*` 和一个 `test-fixtures/*`，确认 SDK 宏可用

### Common Patterns

- 生成宏负责隐藏 `wit-bindgen::generate!`
- `action_ok!` / `action_fail!` / `aggregate_event!` 这类宏适合压缩重复样板
- 若 `wit-bindgen` 无法完全被隐藏，应优先保住“最小接入样板”而不是强行做复杂 proc-macro

<!-- MANUAL: -->
