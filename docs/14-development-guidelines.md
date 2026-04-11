# 开发规范

> 本规范用于约束 Shiroha 的日常开发、设计变更和提交流程。若本规范与其他文档冲突，优先级如下：`00-glossary` > 执行/版本/迁移语义文档 > 本规范 > 其他说明性文档。

## 1. 开发目标

Shiroha 当前处于架构收敛阶段。开发的首要目标不是快速堆功能，而是稳定以下四件事：

- 固定 Host / Guest / Controller / Node 的边界
- 固定 `deployment`、`instance`、`task`、`attempt` 的执行语义
- 保证单机闭环与分布式路径共享同一套语义
- 让后续能力扩展不会破坏已有契约

因此，任何改动都应优先回答两个问题：

1. 这次改动是否引入新的语义分叉？
2. 这次改动是否破坏已有契约、恢复语义或审计能力？

## 2. 基本原则

- **契约优先**：先定义 WIT、数据模型和执行语义，再写实现。
- **单一职责**：每个 crate 只负责一个明确层次，不跨层偷做工作。
- **Effect 驱动**：`machine` 只产出 `Effect`，不直接做 IO 或调度。
- **Host / Guest 分离**：Guest 只表达定义和执行逻辑；Host 负责能力注入、调度、持久化和恢复。
- **部署不可变**：`deployment` 一旦创建即不可修改；升级通过新 deployment 和显式迁移完成。
- **先保守后扩展**：当前只支持宿主内建的 dispatch / aggregation 策略；Guest 自定义策略属于长期规划。
- **文档与实现同步**：改动若触及语义边界，必须同步更新对应文档。

## 3. 术语与文档规则

- 所有开发讨论与文档应使用 [名词表](./00-glossary.md) 中的标准术语。
- 禁止混用 `module`、`deployment`、`instance`。
- 禁止把 `task` / `attempt` 当作业务状态或业务事件来描述。
- 当新增术语、重定义术语、或改变术语边界时，必须先更新 [名词表](./00-glossary.md)。
- 当改动触及以下主题时，必须同步更新对应文档：
  - 执行语义：更新 [11-execution-semantics.md](./11-execution-semantics.md)
  - 版本/升级/迁移：更新 [08-versioning.md](./08-versioning.md) 和 [13-upgrades-and-migrations.md](./13-upgrades-and-migrations.md)
  - 分发/聚合：更新 [06-dispatch.md](./06-dispatch.md)
  - 模块边界：更新 [03-modules.md](./03-modules.md)
  - 路线调整：更新 [99-roadmap.md](./99-roadmap.md)

## 4. 模块边界规范

各模块必须遵守既有分层，不得跨层绕过设计。

### `model`

- 只放纯数据类型与共享枚举。
- 不引入运行时逻辑、网络逻辑或存储逻辑。
- 作为底层共享模型，优先保持零或极少外部依赖。

### `machine`

- 只负责状态机纯逻辑与 `Effect` 生成。
- 不直接访问网络、磁盘、WASM 运行时或节点信息。
- 不直接实现重试、超时、心跳、模块加载等调度行为。

### `engine`

- 只负责 WASM 加载、链接、能力注入和执行。
- 不持有状态机生命周期决策。
- 不绕过 `deployment manifest` 直接解释本地策略。

### `dispatch`

- 负责 task 规划、执行位置选择、重试、取消与聚合。
- 不重写 `machine` 语义，不直接篡改 instance 状态。
- 不引入只在远程路径有效的特殊语义；本地与远程必须共用同一套 task 模型。
- 若复杂度上升，优先在 crate 内拆成 planner / executor adapter / lease-retry coordinator / aggregator，避免形成“调度上帝模块”。

### `transport`

- 只负责控制面/任务面通信抽象。
- 不承载业务决策，不隐式修改调度语义。
- 先围绕第一条参考实现收敛接口，再考虑多传输后端抽象。

### `storage`

- 只负责持久化抽象与实现。
- 不将恢复策略、调度策略写死在存储层。
- 状态快照与事件记录必须携带版本信息；不要默认指望跨版本 replay 解决恢复问题。

### Guest SDK

- `sdk` 负责 Guest 运行时 API。
- `sdk-macros` 只负责编译期样板代码生成。
- 不要把运行时行为偷偷藏进宏里。

## 5. 契约变更规范

以下改动都视为“契约变更”，必须先设计、再实现：

- 修改 WIT world、interface、共享类型或导出入口
- 修改 `deployment manifest` 字段或解释方式
- 修改 `task` / `attempt` 生命周期
- 修改 dispatch / aggregation 规则
- 修改 capability 授权模型
- 修改升级、恢复或迁移语义

契约变更的最低要求：

- 写清楚变更前后行为
- 写清楚兼容性影响
- 写清楚是否影响旧 deployment / 旧 instance / in-flight task
- 同步更新相关文档

若无法在文档中一句话说清楚新语义，说明设计还不够稳定，不应直接编码。

## 6. Rust 代码规范

- 开发环境优先使用仓库提供的 `nix develop`，确保 `rustc`、`cargo`、`protoc`、`pre-commit` 与仓库配置一致。
- 文本格式遵循 `.editorconfig`：默认 4 空格缩进；`nix`、`md`、`yml` 使用 2 空格缩进。
- 使用仓库固定工具链：Rust `1.94.1`，目标包含 `wasm32-wasip2`。
- 新 crate、二进制、库默认使用 workspace 统一版本与依赖，不要在子 crate 随意漂移版本。
- 优先选择显式、可读、可追踪的实现，避免为“优雅”引入额外抽象层。
- 公共边界类型必须命名稳定、含义清楚，避免模糊缩写。
- 错误处理要分层：
  - 可建模的领域错误优先使用结构化错误类型
  - 仅在应用边界或拼装层使用更宽松的错误聚合
- 任何跨进程、跨节点、跨 WIT 边界的数据结构，都必须以兼容性和可序列化为优先考量。

## 7. WIT 与 Guest 开发规范

- WIT 变更默认视为高风险改动。
- 新增 capability 时，必须同时考虑：
  - WIT interface 定义
  - Guest SDK 封装
  - Host 链接注册
  - capability policy 表达方式
  - `deployment manifest` 表达方式
- `definition` 必须暴露 Action / Callback 的执行分类元数据；`Pure` / `Effectful` 的具体规则见 [06-dispatch.md](./06-dispatch.md)。
- capability policy 与执行期 binding 必须分层设计；详细边界见 [05-capability.md](./05-capability.md)。
- `definition` 和 `action` 的职责必须保持清晰，不得把执行期副作用塞进 definition 阶段。
- 过程宏只能减少样板代码，不能引入难以察觉的运行时副作用或隐式约束。

## 8. 调度、恢复与迁移规范

- `task` 是调度层稳定对象，重试时保持同一个 `task_id`。
- `attempt` 代表 task 的一次执行尝试，不得被当作新的 task。
- `running attempt` 禁止热切换到新的 deployment。
- 升级通过“新 deployment + alias 切换 + 显式迁移”完成，不允许原地修改 deployment。
- `pending task` 是否允许迁移，必须由显式兼容规则决定，不能靠“看起来问题不大”。
- 本地执行路径不得偷懒省略 task 持久化模型，否则会导致 Standalone 与分布式语义漂移。
- 恢复入口与 replay 约束以 [11-execution-semantics.md](./11-execution-semantics.md) 为准。

## 9. 测试与质量门槛

提交前至少应确保以下检查通过：

- `just fmt`

CI 发布路径当前还会执行更严格的测试覆盖，开发时若改动影响核心执行语义，建议额外跑：

- `just test`

## 10. 变更提交流程

每次改动尽量满足以下流程：

1. 先确认是否触及契约、术语或分层边界
2. 若触及，先改文档再改实现，或至少同一个提交内同步完成
3. 完成实现后运行最小必要检查
4. 提交说明中写清楚“改了什么”和“为什么这样改”

以下情况不应直接提交实现：

- 术语还没统一
- deployment / task / attempt 语义还说不清楚
- 改动只在远程路径成立，本地路径没有对应模型
- 通过隐藏逻辑绕过既有分层

## 11. 评审关注点

代码评审优先看这些问题：

- 是否破坏了 Host / Guest / Controller / Node 的边界
- 是否引入了本地与远程两套执行语义
- 是否让 deployment 失去不可变性
- 是否让 in-flight task 的恢复或审计变得不可靠
- 是否把副作用偷塞进 `machine`
- 是否在宏、工具函数、helper 中藏了关键行为
- 是否忘了更新相关文档

## 12. 当前阶段明确不做的事

在进入对应 roadmap 阶段前，不主动引入以下内容：

- Guest 自定义 dispatch / aggregation 策略
- 为未来扩展预埋复杂抽象但当前没有使用方的能力层
- 在没有第一条参考实现闭环前，为多传输 / 多存储后端预埋复杂抽象
- 绕过现有语义模型的“临时捷径”
- 没有升级/恢复策略支撑的热更新语义

## 13. 完成定义

一个可接受的开发改动，至少应满足：

- 行为边界清晰
- 分层没有被破坏
- 术语与文档一致
- 本地质量检查通过
- 对恢复、迁移、兼容性没有留下未解释的灰区
