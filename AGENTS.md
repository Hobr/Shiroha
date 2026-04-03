<!-- Generated: 2026-04-02 | Updated: 2026-04-03 -->

# Shiroha

## Purpose

由 WebAssembly 驱动的分布式状态机任务编排框架。将状态机的 Action/Callback 分发到集群节点执行，减轻单机压力。

## Key Files

| File | Description |
| ---- | ----------- |
| `Cargo.toml` | Workspace 根配置，定义所有 crate 成员和共享依赖 |
| `justfile` | 构建/格式化/测试/发布的任务脚本 |
| `flake.nix` | Nix 开发环境配置（Rust 工具链、protoc） |
| `deny.toml` | cargo-deny 依赖审计配置 |
| `.pre-commit-config.yaml` | Pre-commit hooks（fmt、clippy、deny、test） |
| `rust-toolchain.toml` | Rust 工具链版本锁定 |

## Subdirectories

| Directory | Purpose |
| --------- | ------- |
| `app/` | 可执行文件：shirohad 守护进程和 sctl CLI（见 `app/AGENTS.md`） |
| `crate/` | 库 crate：核心类型、引擎、WASM、存储、协议（见 `crate/AGENTS.md`） |
| `docs/` | 架构设计文档（见 `docs/AGENTS.md`） |
| `example/` | 可编译的 `wasm32-wasip2` Flow component 示例（simple / advanced / sub） |

## For AI Agents

### Working In This Directory

- 修改 workspace 依赖后运行 `cargo check --workspace` 确认全局编译通过
- 添加新 crate 需同时更新 `Cargo.toml` 的 `members` 和 `workspace.dependencies`
- 使用 `just fmt` 运行格式化 + pre-commit 全套检查

### Testing Requirements

- `cargo clippy --all-targets --all-features -- -D warnings` 零警告
- `cargo nextest run --all-features --no-tests=warn`
- `just fmt` 通过所有 pre-commit hooks

### Common Patterns

- 所有 crate 使用 `workspace = true` 继承包元信息
- 依赖统一在 workspace 级别声明版本，crate 级别用 `{ workspace = true }` 引用
- edition 2024, resolver 3

## Dependencies

### External

- `tokio` 1.x — 异步运行时
- `wasmtime` 43.x — WASM 运行时
- `tonic` 0.14 — gRPC 框架
- `redb` 3.x — 嵌入式存储
- `serde` / `serde_json` — 序列化
- `tracing` — 结构化日志

### Dependency Graph

```
shirohad ──┬── shiroha-engine ── shiroha-core
           ├── shiroha-wasm ──── shiroha-core
           ├── shiroha-store-redb ── shiroha-core
           └── shiroha-proto

sctl ────── shiroha-proto
```

<!-- MANUAL: -->
