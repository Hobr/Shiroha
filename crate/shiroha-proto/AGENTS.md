<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-02 -->

# shiroha-proto

## Purpose

gRPC 服务定义。使用 protobuf 定义 shirohad 对外的 API 接口，由 tonic-prost-build 在编译时生成 Rust 代码。shirohad 和 sctl 共同依赖此 crate。

## Key Files

| File | Description |
| ---- | ----------- |
| `proto/shiroha.proto` | protobuf 定义：FlowService（3 RPC）+ JobService（8 RPC）+ 全部消息类型 |
| `build.rs` | tonic-prost-build 编译 proto 文件 |
| `src/lib.rs` | `tonic::include_proto!` 导出生成代码 |

## For AI Agents

### Working In This Directory

- 修改 `.proto` 后需 `cargo build -p shiroha-proto` 触发 build.rs 重新生成代码
- 需要系统安装 `protoc`（Nix flake 中已配置 `PROTOC` 环境变量）
- 生成的代码在 `target/` 下，不要手动编辑
- 添加新 RPC 后需同时更新 shirohad 的 service impl 和 sctl 的 client 调用

### Testing Requirements

- `cargo check -p shiroha-proto`
- proto 修改后全量 `cargo check --workspace`（shirohad 和 sctl 都使用生成类型）

<!-- MANUAL: -->
