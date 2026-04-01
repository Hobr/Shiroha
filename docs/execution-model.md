# Shiroha Execution Model

> 工作流执行、状态推进与恢复语义

## Status

- 状态：Draft
- 目标版本：v0.1 基线语义，包含部分 v0.2 分布式扩展
- 当前实现：仓库尚未实现本文中的事件历史、恢复、租约和 Activity 执行链路；本文是目标执行语义说明
- 阅读约定：`必须` / `不能` 表示目标约束；`建议` / `评估` 表示可继续调整的设计点

## 核心概念

| 名词 | 说明 |
| --- | --- |
| `Decision Context` | 状态推进阶段可见的确定性输入集合 |
| `Event History` | 追加式执行历史, 是恢复、重放和审计的权威来源 |
| `State Snapshot` | 从事件历史派生出的缓存, 用于加速恢复和查询 |
| `Command` | 决策函数输出, 例如调度 Activity、设置 Timer、完成或失败 |
| `ActivityTask` | 一个待执行或执行中的 Activity 调度记录 |
| `Timer` | 持久化的延时唤醒机制 |
| `Lease` | 分布式任务的临时执行权 |
| `Late Ack` | 在租约失效、超时或取消后才回传的执行结果 |
| `Signal` | 从外部送入实例的异步事件 |
| `Query` | 对实例状态的只读查询 |

## 确定性边界

- `Decision` 只能访问 `Decision Context`
- `Decision` 不能直接读取 wall-clock time、随机数、文件系统、网络等不可重放外部状态
- 如果工作流逻辑需要时间、随机数或外部观察值, 必须先由 Host 记录为事件
- `Activity` 是非确定性与副作用的唯一承载点
- 同一事件序列 + 同一 Wasm 版本 + 同一确定性上下文, 必须得到同一组 `Command`

## 事件历史与状态快照

- `Event History` 是唯一权威事实来源
- `State Snapshot` 是派生缓存, 仅用于加速恢复和查询
- 快照损坏、丢失或版本不兼容时, 必须能够直接丢弃并基于历史重建
- 快照与历史不一致时, 以历史为准
- 快照生成策略可以逐步优化, 但不能改变事件历史驱动的恢复语义

## 单实例并发控制

- 同一 `Workflow Instance` 在任意时刻只能有一个有效推进者持有写入权
- `ActivityResult`、`Timer`、`Signal`、显式取消等输入必须先进入实例待处理事件集合
- 多个输入可以并发写入待处理队列, 但不能并发驱动同一实例生成多组相互竞争的 `Command`
- 实现上可使用实例版本号、实例租约、CAS 或等价机制, 但对外语义必须保持串行推进
- 恢复场景下也必须遵守同一串行化约束

## 事件排序规则

- 所有影响实例状态的输入都必须先被记录为可排序事件
- 同一实例按持久化后的事件顺序串行处理
- 重放时必须使用同一顺序
- `Late Ack` 等已失去推进资格的结果不进入主事件流, 只进入审计与调试路径

### 分布式场景下的仲裁

v0.1 单进程场景下虽然只有一个本地 `Controller`，但同一实例的多个输入仍可能并发到达，因此依然需要实例级串行化仲裁。v0.2 分布式场景下，多个 `Executor` 可能并发返回 `ActivityResult`，持久化顺序还会进一步受到网络延迟和 Controller 处理时序影响，直接依赖持久化顺序会导致非确定性排序。

正确的流程应为: **串行化仲裁 → 持久化 → 处理**。具体而言:

1. 多个并发输入先进入实例待处理事件集合（如持久化队列）
2. 通过实例锁、CAS 或版本号机制选出当前轮次的唯一输入
3. 选中的输入持久化为主事件流事件
4. 基于该事件推进状态机并生成 `Command`

事件的确定性排序由串行化仲裁机制保证，而非持久化顺序——后者只是仲裁后的结果记录。

## 原子提交语义

一次有效的状态推进必须作为单个原子提交对外成立，至少同时确定:

- 已消费的输入事件
- 新写入的历史事件
- 新生成的 `State Snapshot`
- 新调度的 `ActivityTask` / `Timer` 或与之等价的 outbox 记录

如果底层存储或队列不能天然支持单事务, 实现上可以通过 outbox 等方式达成, 但对外语义仍必须表现为“状态推进与任务/定时器发布属于同一次提交”.

### 持久化与调度分离

v0.1 使用 SQLite 单事务可以覆盖上述全部内容。但需要注意 `TaskQueue` 必须同样持久化到 SQLite 中，不能仅存在于内存——否则进程在事务提交后、任务分发前崩溃，任务就会丢失。

当底层存储或队列不能天然支持单事务时（例如 v0.2 引入独立的持久化任务队列），实现上通过 outbox 模式达成:

1. 状态推进事务中同时写入 outbox 表
2. 独立的后台进程轮询 outbox 并投递到外部队列
3. outbox 投递失败可重试，不影响已提交的状态一致性

## Activity 语义

### 执行模型

- v0.1 以进程内执行为主
- v0.2 开始支持远程 `Executor`
- `Activity` 超时与取消不能只依赖异步包装, 还需要 Wasmtime 中断能力

v0.1 取消机制采用 cooperative 模式: Controller 设置取消标记并通过 Wasmtime `async` cancel 通知，Activity 需要在合适的位置检查取消状态并主动退出。纯计算密集型 Activity 可能无法被及时中断，这是一个已知限制。v0.2 评估引入 fuel metering 作为强制中断的后备手段，但 fuel 中断不保证资源清理，Activity 仍应尽量实现 cooperative 取消。

### 取消、超时与租约

- 取消是协作式的, 不承诺强制 kill
- `task_timeout` 或 `lease` 失效只表示当前 attempt 在控制面上失效, 不代表底层执行一定已停止
- v0.2 中 `Lease` 与 `heartbeat` 用于检测悬挂执行并支持重新领取

### 重试与迟到结果

- `Activity` 默认为 `at-least-once`
- 执行结果至少区分: 成功、可重试失败、不可重试失败、超时、取消
- 控制面只接受当前有效 lease 的首个有效结果用于推进主状态
- 过期 lease 或已取消 attempt 的结果作为 `Late Ack` 进入审计路径, 不再改变主状态

### 幂等与 Activity 上下文

为了让用户真正实现幂等，框架应向 `Activity` 暴露稳定上下文，至少包括:

- `workflow_instance_id`
- `workflow_name`
- `workflow_version`
- `activity_id`
- `task_id`
- `attempt`
- 可直接用于外部系统去重的 `idempotency_key`

框架不保证 Activity 只执行一次，用户必须基于这些稳定标识自行实现幂等。

### Signal 与 Query

`Signal` 和 `Query` 的接口形态与错误处理详见 [接口设计](interfaces.md)。此处仅补充执行语义层面的约束:

- `Signal` 是进入主事件流的外部输入, 必须先持久化为实例输入事件, 再遵守实例内相同的排序与串行推进规则
- 建议按 `workflow_instance_id + signal_name + dedup_key` 作为去重范围; 去重命中时不再次推进主状态
- 当调用方未提供 `dedup_key` 时，默认行为为**不做去重**，允许同一 `workflow_instance_id + signal_name` 下多次投递。需要去重的调用方必须显式提供 `dedup_key`
- `Query` 是只读操作, 不得修改历史、快照或实例状态, 也绝不能发出 `Command`
- 如果 `Query` 需要在 Wasm 中执行, 它应作为可选只读导出进入制品契约

### 工作流级取消

- 取消工作流实例是控制面操作, 与单个 Activity 的取消不同
- 当实例进入取消流程时, 控制面应先记录取消事件, 再向所有在途 Activity 传播取消意图
- `Cancelled` 是实例终态, 但不代表所有底层 Activity 已瞬时停止; 迟到结果仍可能出现
- 默认语义下, 取消不会隐式触发补偿或清理逻辑; 如需补偿, 应由工作流显式建模为状态迁移或专门的补偿 Activity
- 如果某些工作流需要“请求取消后先清理再终态”, 应单独建模为显式取消中间状态, 而不是修改 `Cancelled` 的终态语义

## 实例生命周期

建议至少包含以下状态:

- `Pending`
- `Running`
- `Waiting`
- `Completed`
- `Failed`
- `Cancelled`

其中:

- `Running` 与 `Waiting` 都是未终结运行态
- `Completed`、`Failed`、`Cancelled` 是终态
- 终态不能因 `Late Ack` 改变主状态
- 终态集合不是封闭的，后续版本可能增加（如 `TimedOut`、`Suspended`），具体在对应版本的设计文档中定义

## 故障恢复

### Controller 崩溃

- 重启后从存储加载事件历史和状态快照
- 恢复绑定的 Wasm 版本
- 重放快照后的增量事件
- 重建未完成的 `Task` / `Timer` / `Lease` 视图
- 只有存在待处理事件、到期定时器或显式恢复操作时, 才再次推进状态机

### Executor 崩溃或悬挂

- v0.1 中等同于整个进程崩溃
- v0.2 中通过 `Lease + heartbeat` 检测失效并重新调度
- 新旧 attempt 可能并存, 依赖幂等 Activity 与 `Late Ack` 处理保持正确性

### 已知限制

- v0.1 的 SQLite 是单点
- v0.1 无 split-brain, 但仍可能发生 Activity 重复执行
- v0.2 需要额外处理网络分区、节点失联和重复调度

## 测试策略

- Wasm 单元测试: 提供本地 harness 与 mock host capabilities
- 确定性重放测试: 给定事件历史 + Wasm 版本验证重放一致性
- v0.1 集成测试: 单机端到端 example
- v0.2 分布式测试: 故障、延迟、lease 过期、重复调度
