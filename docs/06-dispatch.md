# 分发与聚合

## 当前范围

- 当前分发策略和聚合策略由宿主内建，并通过 `types.wit` 中的枚举声明
- Controller / Node 当前只保证这些内建策略的执行语义
- **长期规划**：允许 Guest 通过 WASM 提供自定义分发策略和聚合策略；Host 只负责提供统一上下文、候选结果和资源限制。该能力暂不进入当前阶段的实现范围和兼容承诺

## Action 执行分类

为避免多节点执行与外部副作用冲突，Action 在调度层按语义分成两类。该分类必须由 Guest 在定义阶段显式声明，并由 Host 在部署阶段校验：

| 类型 | 特征 | 当前允许策略 |
|------|------|--------------|
| Pure | 显式声明为可复制执行，且经 Host 校验只依赖确定性输入与允许的能力子集 | Local / RemoteAny / RemoteAll(n)；可使用 Majority |
| Effectful | 显式声明为 Effectful，或无法被 Host 验证为 Pure 的 Action | Local / RemoteAny；默认不允许 RemoteAll(n) + Majority，除非未来显式引入幂等声明 |

补充约束：

- `Pure` / `Effectful` 是执行契约，不是调度器在运行时事后猜测的标签
- 若 Action 声明为 `Pure`，但请求 HTTP、KV、时间、随机数或其他非确定性 capability，Host 应在部署时拒绝，或明确按 `Effectful` 处理
- 当前阶段允许 `Pure` Action 依赖的输入只应来自定义中声明的输入 payload、持久化状态快照以及 Host 明确承诺为确定性的能力子集

## 分发策略

用户在状态机定义中声明，决定 Action 在哪里执行：

| 策略 | 行为 |
|------|------|
| Local | 本地 WASM 引擎直接执行 |
| RemoteAny | 发送给任一可用节点 |
| RemoteAll(n) | 发送给 n 个节点并行执行 |

补充约束：

- `RemoteAll(n)` 只适用于 `Pure` Action
- Task 在本地与远程路径上都使用同一套 task 语义，只是执行位置不同
- 远程 task 至少携带：`task_id`、`instance_id`、`deployment_id`、`wasm_hash`、Action 标识、输入 payload、超时信息、attempt 编号、去重键
- Node 以 `deployment_id` 为主键获取执行所需 manifest，以 `wasm_hash` 作为模块缓存索引

## 聚合策略

用户在状态机定义中声明，决定多个结果如何合并：

| 策略 | 行为 |
|------|------|
| First | 取第一个成功的结果 |
| All | 要求全部成功，收集所有结果 |
| Majority | 多数一致的结果胜出 |

补充约束：

- `First` 在得到首个成功结果后，对其余进行 best-effort cancel
- `All` 会保留完整成功/失败明细，由状态机后续决定是否补偿或失败
- `Majority` 仅适用于 `Pure` Action，且“结果一致”以规范化序列化后的字节相等为准；建议配合奇数个并行副本使用，无多数派或出现平票时视为失败

## 故障处理

分发结果携带成功和失败两部分信息，聚合策略可据此做出决策（如 Majority 需知道具体成功/失败数）。

- 重试发生在 task 层，而不是状态机层
- `task_id` 在重试期间保持不变，`attempt` 递增
- Controller 崩溃恢复后，应能根据持久化的 task / attempt 状态继续聚合或重新调度
- 对 `Effectful` Action，默认按至少一次执行语义设计，Guest 若需要精确去重，应依赖幂等键或外部系统的去重能力

## 实现结构建议

为避免 `dispatch` 成为高耦合的巨型模块，实现上建议至少拆成：

- `planner`：决定 task 与副本计划
- `executor adapter`：封装本地 / 远程执行差异
- `lease / retry coordinator`：管理 attempt、租约、超时、重试与取消
- `aggregator`：合并结果并判定完成条件

## WASM 模块分发

节点执行 Action 前需要对应的 WASM 模块：

- Task 引用 `deployment_id`，并携带 `wasm_hash` 作为缓存索引
- 节点本地维护模块缓存，按 hash 索引
- 缓存未命中时向 Controller 拉取模块和对应 manifest
- Controller 充当模块 Registry 与 deployment manifest Registry
