# Phase 1 审计清单

> 当前 Phase 1 状态基线。用于回答“哪些已经闭合，哪些仍未进入 Phase 1”。

## Phase 1 合同

Phase 1 只以 standalone 运行路径为准：

- `shirohad --mode standalone` 在单进程内同时承担 controller 和 node worker
- `sctl` 通过 gRPC 完成 Flow 部署、Job 生命周期管理和事件查询
- `--mode controller` / `--mode node` 仍不是独立可部署模式

## 已闭合的能力

- Flow deploy 静态校验已覆盖不可达状态、死锁/无出口、guest `action` / `guard` / `aggregator` 支持校验，以及 manifest `host-world` 与 component imports 一致性校验
- Job 执行模型已收敛为 Job 级串行锁加 paused 阶段持久化事件队列
- 状态级 hook `on-enter` / `on-exit` 已执行并写入事件日志
- `DispatchMode::Local`、standalone 内的 `DispatchMode::Remote`、以及 standalone 内的 `DispatchMode::FanOut` 已可运行
- `fan-out` 已支持结果收集、guest `aggregate()`、follow-up event 推进，以及 `timeout-ms` / `min-success` 截断
- transition timeout 和 Job `max_lifetime` 已接入 timer wheel
- Job 生命周期 `running` / `paused` / `cancelled` / `completed` 已闭合
- Flow version binding 已实现，旧 Job 始终绑定创建时版本
- redb 已持久化 latest alias、version history、WASM bytes、Job 快照和事件日志
- server 重启后会恢复 Flow registry、module cache、Job 快照、paused 事件队列、transition timeout 和 lifetime timer
- `sctl` 已覆盖 Flow deploy/list/get/version/delete，Job create/list/get/wait/trigger/pause/resume/cancel/delete，以及事件日志查询
- `tracing` 已同时输出结构化 JSON 到终端和按天滚动的日志文件

## 明确不在 Phase 1

- 独立 controller/node 进程部署
- 真实多节点注册、发现、健康检查和跨机器调度
- 自动 `subprocess` 编排、父子 Job 关联和完成回注
- 通用持久化 event inbox
- in-flight Action 跟踪、取消和重启恢复
- OpenTelemetry / Prometheus 指标、分布式追踪
- Join Token / mTLS 节点认证
- 高级调度、能力标签、负载感知
- WASM 权限分层 world 的完整生产化能力

## 当前边界说明

- `state-kind = subprocess` 的配置字段和示例建模已经存在，但 Phase 1 deploy 路径会显式拒绝该状态类型
- standalone 内的 `remote` 和 `fan-out` 都发生在同一进程里；它们验证的是执行边界和运行语义，不是分布式部署能力
- 重启恢复以持久化 Job 快照为边界，不恢复运行中的 in-flight Action
- paused 期间收到的事件会持久化到 Job 快照，但不会作为独立审计事件写入事件流

## 建议验证命令

- `cargo check --workspace`
- `cargo clippy --all-targets --all-features --tests --benches -- -D warnings`
- `cargo nextest run --all-features --no-tests=warn`
- `just test`
