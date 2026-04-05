<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-06 -->

# sctl

## Purpose

Shiroha CLI 管理工具。通过 gRPC 连接 shirohad，提供 Flow 部署和 Job 管理的命令行界面。

## Key Files

| File | Description |
| ---- | ----------- |
| `src/main.rs` | 入口：clap 子命令定义（`flow deploy/ls/get/vers/rm`、`job new/get/ls/trig/pause/resume/cancel/logs/wait/rm`、`complete`） |
| `src/client.rs` | `ShirohaClient`：命令编排、字节输入解码、events follow 和 wait 轮询 |
| `src/flow_presenter.rs` | Flow 文本/JSON 输出 |
| `src/job_presenter.rs` | Job 文本/JSON 输出 |
| `src/event_presenter.rs` | 事件日志文本/JSON 输出 |
| `src/presenter_support.rs` | presenter 共用 JSON / 对齐 / 格式化辅助函数 |
| `build.rs` | shadow-rs 构建信息注入 |
| `Cargo.toml` | CLI 依赖；当前以 shiroha-client 为主，同时保留 clap/tokio/tracing 等运行时依赖 |

## For AI Agents

### Working In This Directory

- 添加新子命令需同时修改 `main.rs`（Commands 枚举）和 `client.rs`（方法实现）
- `sctl` 现在是纯 binary crate，没有 `src/lib.rs`
- CLI 层优先依赖 `shiroha-client` 的领域返回类型，不直接格式化 proto 响应
- 输出逻辑优先放进 presenter 模块；`client.rs` 只保留调用流程和轮询控制
- 纯文本输出使用 `println!` 对齐列，避免 clippy `print_literal` 警告
- `--json` 输出需要保持字段稳定，优先兼容脚本消费
- context / payload 输入统一走 text / hex / file 三种来源解码
- 用户可直接通过 `just sctl ...` 运行 CLI，README 中的示例命令应保持可复制执行

### Testing Requirements

- `cargo check -p sctl`
- `cargo test -p sctl`
- 新增子命令后确认 `--help` 输出正确

<!-- MANUAL: -->
