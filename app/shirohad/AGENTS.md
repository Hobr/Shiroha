<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-02 -->

# shirohad

## Purpose

Shiroha 统一守护进程。Phase 1 仅支持 standalone 模式（Controller + Node 同进程），通过 gRPC 对外提供 FlowService 和 JobService。

## Key Files

| File | Description |
| ---- | ----------- |
| `src/main.rs` | 入口：CLI 参数解析（mode/listen/data-dir）、tracing 初始化、启动服务器 |
| `src/server.rs` | `ShirohaServer` + `ShirohaState`：初始化存储/WASM/定时器，组装 gRPC 服务 |
| `src/flow_service.rs` | gRPC `FlowService` 实现：部署 Flow（WASM 编译→验证→持久化）、查询 |
| `src/job_service.rs` | gRPC `JobService` 实现：创建/触发/暂停/恢复/取消 Job、事件溯源查询 |
| `build.rs` | shadow-rs 构建信息注入 |
| `Cargo.toml` | 依赖所有内部 crate + tonic/clap/tracing |

## For AI Agents

### Working In This Directory

- `ShirohaState` 是核心共享状态，所有 gRPC handler 通过 `Arc<ShirohaState>` 访问
- `trigger_event` 是最关键的路径：查找转移 → 更新状态 → 检查终态 → 注册定时器
- 使用 `Storage` trait 时需导入 `shiroha_core::storage::Storage`（trait 方法需在作用域内）
- 修改 proto 后需重新编译 `shiroha-proto` crate

### Testing Requirements

- `cargo check -p shirohad`
- 修改 gRPC handler 后检查请求/响应类型是否与 proto 一致

### Common Patterns

- gRPC handler 错误映射：`ShirohaError → tonic::Status`
- UUID 字符串解析统一使用 `parse_uuid()` 辅助函数
- 定时器注册：进入新状态后扫描该状态的所有出边，有 timeout 就注册

<!-- MANUAL: -->
