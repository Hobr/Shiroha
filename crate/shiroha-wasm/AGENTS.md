<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-02 -->

# shiroha-wasm

## Purpose

基于 wasmtime 43.x 的 WASM 运行时层。负责模块编译/缓存和 host-guest 桥接。Phase 1 使用 core module ABI，通过线性内存交换 JSON 并调用导出函数。

## Key Files

| File | Description |
| ---- | ----------- |
| `src/runtime.rs` | `WasmRuntime`：封装 wasmtime Engine，提供模块编译入口，开启 fuel 限制 |
| `src/module_cache.rs` | `WasmModule`（模块+哈希）+ `ModuleCache`（按哈希缓存已编译模块） |
| `src/host.rs` | `WasmHost`：host-guest 桥接层，定义 ActionContext/GuardContext，按 Phase 1 ABI 调用 WASM 导出函数 |
| `src/error.rs` | `WasmError` 错误类型 + 到 `ShirohaError` 的转换 |

## For AI Agents

### Working In This Directory

- Phase 1 ABI：guest 需导出 `memory`、`alloc`、`get-manifest` / `invoke-action` / `invoke-guard` / `aggregate`
- host 与 guest 通过线性内存交换 JSON，`i64` 返回值编码为 `(ptr << 32) | len`
- `WasmModule::compute_hash` 当前使用简易哈希（长度+首尾字节），生产应替换为 SHA-256
- wasmtime 43.x 支持 component model，但 Phase 1 使用 core module API

### Testing Requirements

- `cargo check -p shiroha-wasm`
- `cargo test -p shiroha-wasm`

<!-- MANUAL: -->
