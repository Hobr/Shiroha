# 执行语义

## 目标

无论 Action 在本地还是远程执行，系统都应共享同一套 task 生命周期、重试规则和聚合语义，避免 Standalone 与分布式模式出现两套行为。

## 核心对象

- `deployment`：一次不可变部署快照，包含 `wasm_hash`、能力授权结果与执行契约版本
- `instance`：某个状态机实例，创建后绑定一个 `deployment_id`
- `snapshot`：instance 的持久化状态快照，需带状态 schema version 与来源 deployment 信息
- `effect`：状态机产出的执行意图，如 `Execute`、`Persist`、`Complete`
- `task`：对某个 Action/Callback 的一次调度单元
- `attempt`：task 的一次实际执行尝试

## task 生命周期

1. `machine` 产出 `Effect::Execute`
2. `dispatch` 创建 `task` 并持久化为 `pending`
3. 调度器选择本地执行或远程节点执行
4. 执行开始时创建 `attempt`，状态变为 `running`
5. 收到结果后，聚合器更新 `task` 的成功/失败集合
6. 达成聚合条件后，`task` 进入 `succeeded`、`failed`、`cancelled` 或 `timed_out`
7. 聚合结果再反馈回状态机，驱动下一次状态流转

## 重试与幂等

- `Pure` / `Effectful` 的判定与允许策略见 [分发与聚合](./06-dispatch.md)；本节只说明其对重试语义的影响
- `task_id` 在重试期间保持稳定，`attempt` 编号递增
- 远程执行默认按至少一次语义设计，不能假定“只执行一次”
- `Effectful` Action 若需要安全重试，应显式依赖幂等键、外部系统去重机制，或限制为单点执行
- 是否重试由调度层策略决定，不由 Guest 在运行时临时改变
- 重试只允许发生在同一个 `deployment_id` 内；若要切到新 deployment，应走显式 `task migration`

## 超时与取消

- 每个 task 必须带 deadline 或 timeout
- 超时会终止当前 attempt，并由调度层决定重试、降级还是整体失败
- `First` 聚合策略在取得首个成功结果后，会对其余运行中的 attempt 发起 best-effort cancel
- cancel 是调度层语义，不保证节点一定在用户代码尚未执行前停止
- 已经开始执行的 `running attempt` 不允许热切换到另一份 WASM；升级时只能等待其完成，或取消后在新 deployment 下重建 task

## 恢复

- Controller 崩溃恢复后，应从持久化状态重建未完成 task
- 长时间无心跳或无结果的 `running` attempt，应在租约过期后视为丢失
- 本地执行路径在 Standalone 模式下也应沿用相同的 task 持久化模型，避免测试环境与生产语义漂移
- 默认恢复入口应是最近 snapshot + task / attempt 状态，而不是跨 deployment replay 旧 event log
- 恢复时必须按 task 当时绑定的 `deployment_id` 继续判断，不得在恢复过程中静默切换到新的 deployment

## 聚合比较

- `First`：首个成功结果胜出，其余结果只用于审计
- `All`：保留全部结果与失败明细，供状态机或上层策略决策
- `Majority`：仅适用于 `Pure` Action，结果相等性以规范化序列化后的字节相等为准

## 非目标

- 当前阶段不支持 Guest 在 WASM 中自定义 dispatch / aggregation 算法
- 当前阶段不承诺跨不同执行契约版本共享同一条 in-flight task 恢复路径
