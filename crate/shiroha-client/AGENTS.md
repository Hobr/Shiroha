<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-06 | Updated: 2026-04-06 -->

# shiroha-client

## Purpose

Shiroha 客户端抽象层。封装 tonic gRPC client，把 proto 请求/响应转换为更清晰的领域返回类型，供 `sctl` 或未来 Web/Desktop 交互端复用。

## Key Files

| File | Description |
| ---- | ----------- |
| `src/client.rs` | `ControlClient`：封装 FlowServiceClient / JobServiceClient 连接 |
| `src/flow.rs` | Flow 相关接口与领域类型：`FlowDetails`、`FlowVersionSummary`、强制删除结果 |
| `src/job.rs` | Job 相关接口与领域类型：`JobDetails`、`JobEvent`、`EventQuery` |
| `src/manifest.rs` | manifest / event JSON 解析与辅助函数 |
| `src/lib.rs` | 对外导出公共 API |
| `Cargo.toml` | 依赖 shiroha-proto + tonic + serde_json + anyhow |

## For AI Agents

### Working In This Directory

- 这里负责“调用协议”和“领域建模”，不负责 CLI 文本格式化
- 新增查询接口时优先返回领域类型，而不是直接暴露 proto 类型
- 若服务端字段是 JSON 字符串，优先在这里解析成 `serde_json::Value`
- `sctl` 和未来其他交互端应尽量只依赖这里导出的类型

### Testing Requirements

- `cargo check -p shiroha-client`
- `cargo test -p shiroha-client`
- 修改公开返回类型后，至少确认 `sctl` 仍能编译通过

### Common Patterns

- 读操作优先做排序/去重，避免把服务端内部顺序泄漏给调用方
- `TryFrom<proto>` 适合有 JSON 解析或字段转换的类型
- 删除/触发/暂停/恢复/取消这类命令型接口可继续返回轻量结果或 `()`

<!-- MANUAL: -->
