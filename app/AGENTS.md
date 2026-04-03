<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-03 -->

# app

## Purpose

可执行文件目录，包含框架的两个二进制程序：守护进程 shirohad 和管理工具 sctl。

## Subdirectories

| Directory | Purpose |
| --------- | ------- |
| `shirohad/` | 统一守护进程，支持 standalone/controller/node 模式（见 `shirohad/AGENTS.md`） |
| `sctl/` | CLI 管理工具，通过 gRPC 连接 shirohad（见 `sctl/AGENTS.md`） |

## For AI Agents

### Working In This Directory

- 两个 app 共享 `shiroha-proto` 的 gRPC 类型定义
- 修改 proto 后需同时检查 shirohad 和 sctl 的编译
- `sctl` 同时维护人类可读输出和 `--json` 机器可读输出
- `build.rs` 使用 shadow-rs 注入构建信息

<!-- MANUAL: -->
