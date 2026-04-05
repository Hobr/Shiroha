<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-06 -->

# shirohad

## Purpose

Shiroha 统一守护进程。Phase 1 仅支持 standalone 模式（Controller + Node 同进程），通过 gRPC 对外提供 FlowService 和 JobService。

## Key Files

| File | Description |
| ---- | ----------- |
| `src/main.rs` | 入口：CLI 参数解析（mode/listen/data-dir）、tracing 初始化、启动服务器；当前不同 mode 仍共享同一套启动路径 |
| `src/server.rs` | `ShirohaServer` + `ShirohaState`：初始化存储/WASM/定时器，重载版本化 Flow/WASM 缓存，组装 gRPC 服务 |
| `src/flow_service.rs` | gRPC `FlowService` 实现：部署 Flow（WASM 编译→验证→持久化 latest/versioned flow + wasm bytes）、查询 |
| `src/job_service.rs` | gRPC `JobService` 实现：创建/触发/暂停/恢复/取消 Job、状态 hook、事件溯源查询 |
| `src/grpc_tests.rs` | 真实 tonic client/server 往返测试，覆盖 example wasm 组件 |
| `src/test_support.rs` | 测试夹具：临时数据目录、UDS server、fixture/example wasm 构建 |
| `build.rs` | shadow-rs 构建信息注入 |
| `Cargo.toml` | 依赖所有内部 crate + tonic/clap/tracing |

## For AI Agents

### Working In This Directory

- Phase 1 只有 standalone 路径是完整可用的；`--mode controller/node` 当前仍复用同一启动逻辑
- `ShirohaState` 同时维护 latest flow 注册表、versioned flow 注册表、latest engine 和 versioned engine
- `trigger_event` 是最关键的路径：查找绑定版本的转移 → 更新状态 → 执行 `on-exit` / transition action / `on-enter` → 检查终态 → 注册定时器
- Flow 部署时会额外持久化原始 WASM 字节，server 重启后会重建 module cache
- 当前重启恢复会重载 Flow/WASM/Job 快照；暂停期间排队事件会随 Job 快照一起恢复，但运行中的定时器仍不会恢复
- 使用 `Storage` trait 时需导入 `shiroha_core::storage::Storage`（trait 方法需在作用域内）
- 修改 proto 后需重新编译 `shiroha-proto` crate
- README 和 `just shirohad ...` 是当前推荐的用户启动入口，变更参数行为后要同步更新

### Testing Requirements

- `cargo check -p shirohad`
- `cargo test -p shirohad`
- 修改 gRPC handler 后检查请求/响应类型是否与 proto 一致
- 修改恢复、暂停队列或定时器逻辑后，优先补 `server.rs` / `job_service.rs` 里的重启与 timeout 测试

### Common Patterns

- gRPC handler 错误映射：`ShirohaError → tonic::Status`
- UUID 字符串解析统一使用 `parse_uuid()` 辅助函数
- 定时器注册：进入新状态后扫描该状态的所有出边，有 timeout 就注册
- 运行中 Job 必须按 `job.flow_version` 读取 Flow/WASM，不能回退到最新版本
- `DispatchMode::Remote` 在当前 standalone 里会退化为同进程调用；`FanOut` 仍未在运行时真正执行

<!-- MANUAL: -->
