# 事件溯源

每次状态转移记录为不可变事件日志，天然适合状态机场景。
当前 Phase 1 的事件模型更接近“运行时状态推进日志”，而不是一个覆盖所有入口与因果细节的完整审计系统。

## 事件记录

当前事件流主要包含：`Created`、`Transition`、`ActionComplete`、`Paused`、`Resumed`、`Cancelled`、`Completed`。

补充说明：

- `Transition` 只记录已经提交的状态转移
- `ActionComplete` 记录 action 名、执行状态，以及 fan-out / remote 路径上的可选 `node_id`
- fan-out 聚合返回的 follow-up event 不会生成单独的“aggregate completed”事件；后续只会表现为普通的 `Transition`

## 作用

- **审计追踪**：当前可以记录 Job 生命周期、状态转移和 action 完成结果
- **故障恢复**：Controller 重启后恢复持久化的 Job 快照、Flow 版本、WASM 模块，以及 Job 的暂停事件队列和 timeout 计划
- **恢复边界**：当前不会从事件日志或宿主句柄层面恢复 in-flight Action 执行
- **调试**：回放事件序列定位问题
- **分析**：统计状态停留时间、转移频率、失败分布

## 实现

- 事件日志写入 Storage（与 Job 状态同一后端）
- 当前 redb 后端会把 Job 快照与生命周期事件放进同一个写事务中，保证一致性
- Phase 1 已提供按 Job 查询事件日志的 API
- 事件回放重建仍属于后续迭代，当前运行时恢复主要依赖持久化的 Job 快照、暂停事件队列、timeout 计划和版本化 Flow/WASM 注册表

当前限制：

- 没有单独的“外部 trigger-event 被接收”事件类型
- 没有记录调用方身份、事件来源、原始 payload 摘要、guard reject、排队原因或取消原因
- `paused` 状态下排队的事件会持久化到 Job 快照，但不会作为独立事件写入事件流
- `ActionComplete` 当前只保留 `status` 和可选 `node_id`，不会持久化 guest `output`
