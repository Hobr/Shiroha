# 核心 Domain 模型 (shiroha-core)

## 角色

`shiroha-core` 是整个项目的 domain 词汇表。它定义 FSM 是什么、Action 怎么被引用、分发策略与聚合策略有哪些类别。它**不接触任何 I/O**——不读 WASM、不发 RPC、不写存储——因此可以被任意上层 crate 复用而不引入传染性依赖。

任何在 core 之外有意义的"概念",都不属于 core;任何 core 之外的 crate 想要新增概念,先问"这是否是 FSM 模型本身的一部分"。

## 概念清单

### FSM

- 一份 FSM 定义包含若干 State 与若干 TransitionRule
- 每条 TransitionRule 描述从一个 State 在特定输入下迁移到另一个 State
- 每个 State 可附带 on-enter / on-exit 的 Action 引用列表。**on-exit 默认按声明顺序串行执行**(因为退出动作常有顺序依赖,如先释放资源再清理锁);**on-enter 默认并发执行**。若 FSM 作者需要调整,应在 ActionRef 上声明依赖关系(具体机制在 WIT 描述符中定义)
- 每条 TransitionRule 可附带一个 Action 引用,其执行结果作为决策输入

### Action 引用 (ActionRef)

- core 中的 Action 仅作为**引用**存在,不含实现
- 实现位于 WASM 组件中,由 `shiroha-wasm` 按引用查找并调用
- 一个 ActionRef 至少包含:名称、所属 ComponentId、分发策略、签名提示、WaitingMode、aggregation_timeout(聚合超时,适用于所有 Aggregation 类型;未声明时使用全局默认值)

### WaitingMode

由 FSM 作者在 ActionRef 上声明,决定派发期间 Job 所处的态:

- `Blocking` — Job 留在转移源状态,Engine 在原地 await Aggregator;事件流紧凑,适合短 Action
- `Waiting` — 派发瞬间 Job 进入 `Waiting` 中间态,结果回流后触发转移;状态显式可观测,适合长 Action

Blocking / Waiting 由 ActionRef 逐条声明,Engine 不强制全局策略——同一 FSM 内不同 Action 可混用。

选型指导:若 Action 不能保证幂等(如发送邮件、扣款),**必须**使用 `Waiting`。原因见 `architecture.md` 的"Action 幂等性"一节。

### DispatchPolicy

core 只暴露两个变体,具体路由发生在 `shiroha-dispatch`:

- **Local** — 在主控进程内执行(本地 Executor)
- **Remote(selector, aggregation)** — 派发到 `selector` 选出的若干节点,结果按 `aggregation` 合并

`NodeSelector` 决定"派给哪些节点",内置形态包括:`one(filter)`(选 1 个)、`n(count, filter)`(选 N 个)、`all(filter)`(选全部健康的);filter 按标签 / 地域 / 健康度筛选。

当 selector 返回单个节点时,`aggregation` 被忽略——代码上仍要传(枚举一致),实际不调用聚合。

### Executor trait

core 定义 `Executor` trait 作为 Action 执行的抽象。该 trait 描述**语义契约**:给定 ComponentId + ActionRef + 输入原始字节,返回输出原始字节或执行错误。执行错误分为两类:网络型(可重试)与业务型(透传给 Aggregator)。

`shiroha-wasm` 实现此 trait 提供 `LocalExecutor`;`shiroha-dispatch` 依赖此 trait 而非直接依赖 `shiroha-wasm`,从而解耦执行位置与分发逻辑。v0.3 中 engine 直接调用 LocalExecutor;v0.5 引入 dispatch 层后,executor 通过 trait 多态调度。

> **core 的零 I/O 原则** — Executor trait 是纯 domain 抽象,定义"执行是什么",不定义"如何执行"。具体的异步运行时、网络调用等实现细节由上层 crate(wasm / transport / dispatch)承担。

### Aggregation

聚合策略将多个执行结果合并为一个供 FSM 使用的输入。core 提供分类:

- **First** — 任一节点成功即返回,其余取消或忽略
- **AllOk** — 全部成功才算成功;任一失败即整体失败
- **Quorum(k)** — 至少 k 个相同(或可比)结果才返回
- **Custom** — 委托给 WASM 中用户定义的聚合函数(见 `wit-interfaces.md`)。超时值由 ActionRef 上的 `aggregation_timeout` 字段指定;未声明时使用全局默认值

### ComponentId 与 Job

- **ComponentId** — 一份具体 WASM 组件字节的不透明标识(主控按内容 hash 派生)。所有跨主从边界的引用都用 ComponentId,`worker` / `dispatch` / `transport` / `wasm` 不感知"版本"或"Flow"概念
- **Job** — 一次具体运行实例,持有 ComponentId、当前状态、累积事件

**Flow**(FSM 定义版本的人类语义命名)只在主控层(`shiroha-storage` / `shiroha-engine` / `shiroha-control`)存在,详见 `storage.md`。core 自身不引入 Flow 类型,以避免下游 crate 被版本概念污染。

## 不在本 crate 内的内容

为避免污染 domain 层,以下职责必须放在其他 crate:

- 任何异步运行时、网络 I/O、磁盘操作(Executor trait 的实现属于上层,不属于 core)
- WASM 加载或组件实例化
- 持久化、序列化到磁盘 (但 core 类型本身需可被序列化)
- 节点注册表、连接池
- 具体的错误类型实现(core 只定义错误分类的语义,不定义具体枚举)

## 与其它 crate 的契约

- `shiroha-wit`:WIT 中导出的"FSM 描述符"类型必须能映射到 core 的 FSM 类型
- `shiroha-wasm`:实现 core 的 `Executor` trait(LocalExecutor)
- `shiroha-dispatch`:基于 core 的 `Executor` trait / DispatchPolicy / Aggregation 进行路由与聚合
- `shiroha-engine`:基于 core 的 FSM 模型驱动状态转移;v0.3 直接调 Executor,v0.5 经 dispatch
- `shiroha-storage`:Job / Event 的持久化结构必须能往返 core 模型;Flow / Component 由 storage 自行建模(不在 core)

跨 crate 字段或枚举变更属于破坏性改动,需要在 PR 中说明影响范围。
