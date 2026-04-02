<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-02 -->

# sctl

## Purpose

Shiroha CLI 管理工具。通过 gRPC 连接 shirohad，提供 Flow 部署和 Job 管理的命令行界面。

## Key Files

| File | Description |
| ---- | ----------- |
| `src/main.rs` | 入口：clap 子命令定义（deploy/flows/create/get/trigger/pause/resume/cancel/events） |
| `src/client.rs` | `ShirohaClient`：gRPC 客户端封装，每个子命令对应一个方法 |
| `build.rs` | shadow-rs 构建信息注入 |
| `Cargo.toml` | 依赖 shiroha-proto + tonic/clap |

## For AI Agents

### Working In This Directory

- 添加新子命令需同时修改 `main.rs`（Commands 枚举）和 `client.rs`（方法实现）
- 不依赖 shiroha-core/engine/wasm/store — 只通过 proto 类型与 shirohad 通信
- 输出格式使用 `println!` 对齐列，避免 clippy `print_literal` 警告

### Testing Requirements

- `cargo check -p sctl`
- 新增子命令后确认 `--help` 输出正确

<!-- MANUAL: -->
