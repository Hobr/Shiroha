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
| `src/host/network_support.rs` | reqwest 驱动的 `net` host import，实现 HTTP client 配置与请求执行 |
| `src/host/storage_support.rs` | `store` host import，实现命名空间 KV 读写；`shirohad` 路径下注入真实存储后端 |
| `src/error.rs` | `WasmError` 错误类型 + 到 `ShirohaError` 的转换 |
| `../shiroha-wit/wit/flow.wit` | 基础 `world flow`，只导出 Flow manifest / action / guard / aggregate |
| `../shiroha-wit/wit/net.wit` | 独立的 HTTP capability interface，定义 client/request/response/TLS/proxy 结构 |
| `../shiroha-wit/wit/network-flow.wit` | `world network-flow`：`include flow` + `import net`，用于需要 HTTP 的 guest |
| `../shiroha-wit/wit/store.wit` | 独立的 KV capability interface，定义 get/put/delete/list-keys |
| `../shiroha-wit/wit/storage-flow.wit` | `world storage-flow`：`include flow` + `import store` |
| `../shiroha-wit/wit/full-flow.wit` | `world full-flow`：`include flow` + `import net` + `import store` |

## For AI Agents

### Working In This Directory

- component guest 默认实现 `crate/shiroha-wit/wit/flow.wit` 的 `world flow`；需要额外能力时选择 `network-flow` / `storage-flow` / `full-flow`
- `net.wit` 和 `store.wit` 都是独立 capability，新增字段或类型时要同时考虑 host 映射和 guest 侧 `wit-bindgen` 可用性
- 当前已经拆出 `world flow` / `network-flow` / `storage-flow` / `full-flow`；但文档里提到的更细粒度权限体系仍未落地
- `WasmModule::compute_hash` 现在使用 SHA-256 内容哈希，避免弱哈希导致的缓存碰撞
- component guest 实例化时使用 `wasmtime_wasi::p2::add_to_linker_sync` 提供 WASI imports
- 当前 host 通过 reqwest 提供 `net.send`；配置映射优先保持结构化 WIT，而不是退化为 JSON 字符串黑盒
- 当前 host 通过 capability store trait 提供 `store`；`WasmHost::new()` 默认落到内存 store，`shirohad` 会显式注入真实存储后端
- deploy 期的 component import 校验在 `shirohad` 层，不在 `shiroha-wasm` 层；修改 world 组合时要同步考虑两边
- `aggregate()` host 调用已打通；当前 standalone 运行时会在 fan-out 结果收集后调用它，并把返回事件交回 `shirohad` 继续推进状态机

### Testing Requirements

- `cargo check -p shiroha-wasm`
- `cargo test -p shiroha-wasm`

<!-- MANUAL: -->
