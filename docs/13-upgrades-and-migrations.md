# 升级与迁移

## 目标

在不破坏恢复语义、缓存一致性和审计能力的前提下，让系统能够从旧版本 WASM 平滑切换到新版本。

## 基本原则

- `deployment` 必须不可变
- 升级通过创建新的 `deployment_id` 完成，而不是修改原有 deployment
- 可变的是“默认流量指向”与“现有对象绑定关系”，而不是 deployment 快照本身
- 运行中的 `attempt` 不允许热切换到另一份 WASM

## 为什么 deployment 必须不可变

- 同一个 `deployment_id` 必须始终对应同一份 `wasm_hash`、能力授权结果和执行契约版本
- Node 的模块缓存、Controller 的恢复逻辑和审计记录都依赖这个稳定映射
- 若原地修改 deployment，系统将无法可靠判断某个 in-flight task 到底运行的是旧代码还是新代码
- 回滚、问题复现和结果比对也都要求 deployment 可复现

## 可变对象

升级时允许变化的对象有三类：

- `release alias`：决定“新建 instance 默认绑定哪个 deployment”
- `instance` 绑定：现有状态机实例可显式迁移到新 `deployment_id`
- `task` 绑定：尚未完成的 task 可在满足条件时迁移到新 `deployment_id`

## 推荐升级流程

1. 基于新的 WASM 产出新 `deployment`
2. 完成兼容性检查
3. 将 `release alias` 切到新的 `deployment_id`
4. 评估是否迁移旧 `instance` 与旧 `task`
5. 待旧 deployment 无活跃引用后，再执行清理

## 对不同对象的处理规则

| 对象 | 是否允许迁移 | 规则 |
|------|--------------|------|
| 新建 instance | 是 | 直接绑定 `release alias` 当前指向的新 deployment |
| `pending task` | 有条件允许 | 仅在输入、Action 标识、执行契约和所需能力仍兼容时迁移 |
| `running attempt` | 否 | 不允许热切换代码；要么等其完成，要么取消后基于新 deployment 重建 task |
| 已完成 task | 否 | 作为历史记录保留，不回写到新 deployment |
| 运行中 instance | 有条件允许 | 必须走显式 `instance migration`，不能静默切换 |

## 兼容性分级

### Level 1：实现兼容

变更只影响内部实现，不改变下列内容：

- 状态 schema
- Action 执行分类（`Pure` / `Effectful` 或等价语义）
- Action / Callback 标识
- 输入输出结构
- 所需 capability 集
- WIT / 执行契约版本

处理建议：

- 新建 instance 直接走新 deployment
- `pending task` 可迁移
- `running attempt` 仍然不热切换
- 现有 instance 可选择继续跑旧 deployment，或显式切换到新 deployment

### Level 2：状态兼容

变更触及状态结构或执行路径，但可以通过确定性的状态转换完成迁移。

处理建议：

- 必须定义 `instance migration`
- 迁移逻辑应优先基于持久化 snapshot 做确定性转换，而不是默认依赖跨版本 replay 历史 event log
- 迁移前冻结该 instance 的新 task 创建
- 迁移完成后再允许其继续推进

### Level 3：不兼容升级

变更破坏状态结构、WIT 契约、能力模型或结果解释方式。

处理建议：

- 不迁移现有 running instance
- 旧 instance 继续留在旧 deployment 直至结束，或由运维显式终止
- 新流量全部走新 deployment

## task 迁移规则

`task migration` 只适用于尚未完成的 task，并且需要满足以下条件：

- 目标 deployment 中存在同名 Action / Callback
- Action 执行分类与复制执行约束仍兼容
- 输入 payload 仍可被新 deployment 正确解释
- 旧 task 的能力需求在新 deployment 中仍被授权
- 聚合策略与结果解释规则未发生不兼容变化

迁移后应保留：

- 原 `task_id`
- 原审计链路
- 迁移前后 deployment 的关联记录

迁移后不应保留：

- 旧 deployment 下已经开始执行的 `running attempt`

## instance 迁移规则

`instance migration` 应视为显式运维操作，而不是普通重试行为。

最低要求：

- 迁移前持久化当前 instance 状态
- 迁移过程中阻止并发推进
- 记录从旧 `deployment_id` 到新 `deployment_id` 的迁移事件
- 若迁移失败，应能回退到旧 deployment 继续运行

## 状态与事件版本化要求

详细持久化与恢复语义见 [传输层与持久化](./09-transport-storage.md) 与 [执行语义](./11-execution-semantics.md)。

迁移额外要求：

- migration 默认应以“读取旧 snapshot -> 运行显式迁移逻辑 -> 写入新 snapshot”的方式建模
- 若某个迁移方案依赖 event replay，必须单独声明兼容范围并测试

## 非目标

- 在同一个 `deployment_id` 上原地替换 WASM
- 对已经开始执行的 `attempt` 做热补丁或热切换
- 自动推断所有 instance 都可以安全迁移

## 当前建议

- 把 deployment 视为不可变快照
- 把升级建模为“新 deployment + alias 切换 + 显式迁移”
- 先只支持最保守的迁移策略：
  - `pending task` 可迁移
  - `running attempt` 不迁移
  - `instance migration` 必须显式触发
