<!-- Generated: 2026-04-02 | Updated: 2026-04-08 -->

# Shiroha

## Purpose

由 WebAssembly component 驱动的分布式状态机任务编排框架。当前可用实现仍以 Phase 1 的 standalone 路径为主：`shirohad` 在单进程内同时承担控制面和执行面，`sctl` 通过 gRPC 进行 Flow 部署、Job 生命周期管理和事件查询。

## Key Files

| File | Description |
| ---- | ----------- |
| `Cargo.toml` | Workspace 根配置，定义 10 个成员和共享依赖版本 |
| `README.md` | 项目入口说明，包含 Quick Start、CLI 示例和开发命令 |
| `justfile` | 常用构建、示例编译、格式化、测试、文档和发布入口 |
| `flake.nix` | Nix 开发环境配置（Rust 工具链、protoc 等） |
| `deny.toml` | cargo-deny 依赖审计配置 |
| `rust-toolchain.toml` | Rust 工具链版本锁定 |

## Subdirectories

| Directory | Purpose |
| --------- | ------- |
| `app/` | 可执行文件：`shirohad` 守护进程和 `sctl` CLI（见 `app/AGENTS.md`） |
| `crate/` | 库 crate：core、engine、proto、client、WASM、redb store、guest SDK、canonical WIT（见 `crate/AGENTS.md`） |
| `docs/` | 架构、调度、安全、运维和路线图文档（见 `docs/AGENTS.md`） |
| `example/` | `wasm32-wasip2` Flow component 示例（`simple` / `advanced` / `warning-deadlock` / `sub`） |

## For AI Agents

### Working In This Directory

- 当前完整可跑通的是 standalone 运行路径；`shirohad --mode controller/node` 仍复用同一套服务端实现，不要把它们当成已完成的分布式模式。
- 修改 workspace 依赖、crate 成员或公共类型后，至少运行 `cargo check --workspace`；涉及 public API、proto、WIT 或 shared model 时优先做全 workspace 验证。
- 需要核对 workspace 实际成员时，优先用 `cargo metadata --no-deps --format-version 1`，不要手数目录。
- 新增 crate 需同时更新 `Cargo.toml` 的 `members` 和 `workspace.dependencies`。
- 面向用户的入口、参数和典型命令优先同步到 `README.md` 和 `justfile`，不要只留在测试、fixture 或示例目录。

### Testing Requirements

- `cargo check --workspace`
- `cargo clippy --all-targets --all-features --tests --benches -- -D warnings`
- `cargo nextest run --all-features --no-tests=warn`
- `just test` 会额外带上 `--run-ignored all`，用于重型 restart / integration smoke
- `just fmt` 会跑 `cargo fmt` 和 pre-commit hooks

### Common Patterns

- workspace 当前确认有 10 个成员：`sctl`、`shirohad`、`shiroha-client`、`shiroha-core`、`shiroha-engine`、`shiroha-proto`、`shiroha-sdk`、`shiroha-store-redb`、`shiroha-wasm`、`shiroha-wit`。
- 所有 crate 使用 `workspace = true` 继承包元信息；依赖版本统一在 workspace 根声明，成员 crate 用 `{ workspace = true }` 引用。
- Flow 部署会同时持久化 latest registration、versioned registration 和原始 wasm bytes；运行中的 Job 始终绑定 `job.flow_version`，不能偷偷回退到最新版本。
- Job 快照会持久化 `pending_events`、`scheduled_timeouts`、`timeout_anchor_ms`、`max_lifetime_ms`、`lifetime_deadline_ms`，server 重启后会恢复 Flow registry、module cache 和可恢复定时器。
- `crate/shiroha-wit` 是 canonical WIT 来源；`shiroha-sdk` build script、示例 component 和宿主侧测试都依赖它，改 world/interface 时必须联动检查这些路径。
- deploy 期的 component import/world 一致性校验在 `app/shirohad/src/flow_service.rs`，不是在 `crate/shiroha-wasm` 内部自动完成。
- `DispatchMode::Remote` 在 standalone 中会通过 in-process transport 进入同进程 node worker；`DispatchMode::FanOut` 已支持同进程 fan-out 槽位分发、结果收集、guest `aggregate()` 调用和 follow-up event 推进。

## Dependencies

### External

- `tokio` 1.x：异步运行时
- `wasmtime` / `wasmtime-wasi` 43.x：component model 运行时
- `tonic` 0.14：gRPC 框架
- `redb` 4.x：嵌入式存储
- `serde` / `serde_json`：序列化
- `tracing` / `tracing-subscriber`：结构化日志
- `reqwest` 0.12：WASM `net` capability host 实现

### Dependency Graph

```text
shirohad ──┬── shiroha-engine ── shiroha-core
           ├── shiroha-wasm ──── shiroha-core
           ├── shiroha-store-redb ── shiroha-core
           └── shiroha-proto

sctl ────── shiroha-client ───── shiroha-proto

shiroha-sdk ──(build-dep)── shiroha-wit
```

<!-- MANUAL: -->
