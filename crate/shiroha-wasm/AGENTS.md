<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-06 -->

# shiroha-wasm

## Purpose

基于 wasmtime 43.x 的 WASM 运行时层。负责 component 编译/缓存和 host-guest 桥接。当前仅支持 component/wasip2 typed export 路线。

## Key Files

| File | Description |
| ---- | ----------- |
| `src/runtime.rs` | `WasmRuntime`：封装 wasmtime Engine，提供 component 编译入口，开启 fuel 并启用 component model |
| `src/module_cache.rs` | `WasmModule`（component + 哈希）+ `ModuleCache`（按哈希缓存已编译 component） |
| `src/host.rs` | `WasmHost`：host-guest 桥接层，通过 component typed exports 调用 guest |
| `src/error.rs` | `WasmError` 错误类型 + 到 `ShirohaError` 的转换 |
| `wit/flow.wit` | component guest 的规范 world，定义 manifest / action / guard / aggregate 的 WIT 结构 |

## For AI Agents

### Working In This Directory

- component guest 需按 `wit/flow.wit` 导出同名 typed functions；host 通过 `wasmtime::component::Instance::get_typed_func` 调用
- `wit/flow.wit` 现在除了 guest exports，还包含 host 提供的 `network` import；新增字段或类型时要同时考虑 guest 侧 `wit-bindgen` 可用性
- 当前只实现单一 `world flow`；文档里提到的 `sandbox/network/storage/full` world 和权限匹配还没有落地
- `WasmModule::compute_hash` 当前使用简易哈希（长度+首尾字节），生产应替换为 SHA-256
- component guest 实例化时使用 `wasmtime_wasi::p2::add_to_linker_sync` 提供 WASI imports
- 当前 host 还额外通过 reqwest 提供 `network.send`；配置映射优先保持结构化 WIT，而不是退化为 JSON 字符串黑盒
- `aggregate()` host 调用已打通，但 standalone 运行时还没有真正执行 fan-out 调度链路

### Testing Requirements

- `cargo check -p shiroha-wasm`
- `cargo test -p shiroha-wasm`

<!-- MANUAL: -->
