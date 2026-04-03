<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-03 -->

# sctl

## Purpose

Shiroha CLI 管理工具。通过 gRPC 连接 shirohad，提供 Flow 部署和 Job 管理的命令行界面。

## Key Files

| File | Description |
| ---- | ----------- |
| `src/main.rs` | 入口：clap 子命令定义（deploy/flows/flow/create/get/jobs/trigger/pause/resume/cancel/events/wait） |
| `src/client.rs` | `ShirohaClient`：gRPC 客户端封装，负责文本/JSON 输出、字节输入解码、events follow 和 wait 轮询 |
| `build.rs` | shadow-rs 构建信息注入 |
| `Cargo.toml` | 依赖 shiroha-proto + tonic/clap/serde_json |

## For AI Agents

### Working In This Directory

- 添加新子命令需同时修改 `main.rs`（Commands 枚举）和 `client.rs`（方法实现）
- 不依赖 shiroha-core/engine/wasm/store — 只通过 proto 类型与 shirohad 通信
- 纯文本输出使用 `println!` 对齐列，避免 clippy `print_literal` 警告
- `--json` 输出需要保持字段稳定，优先兼容脚本消费
- context / payload 输入统一走 text / hex / file 三种来源解码

### Testing Requirements

- `cargo check -p sctl`
- `cargo test -p sctl`
- 新增子命令后确认 `--help` 输出正确

<!-- MANUAL: -->
