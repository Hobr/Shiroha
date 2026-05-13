# 核心 Domain 模型 (shiroha-core)

## 角色

`shiroha-core` 是整个项目的 domain 词汇表。它定义 FSM 是什么、Action 怎么被引用、分发策略与聚合策略有哪些类别。它**不接触任何 I/O**——不读 WASM、不发 RPC、不写存储——因此可以被任意上层 crate 复用而不引入传染性依赖。

任何在 core 之外有意义的"概念",都不属于 core;任何 core 之外的 crate 想要新增概念,先问"这是否是 FSM 模型本身的一部分"。

## 概念清单

### FSM

- 一份 FSM 定义包含若干 State 与若干 TransitionRule
- 每条 TransitionRule 描述从一个 State 在特定输入下迁移到另一个 State
- 每个 State 可附带 on-enter / on-exit 的 Action 引用列表
- 每条 TransitionRule 可附带一个 Action 引用,其执行结果作为决策输入

### Action 引用 (ActionRef)

- core 中的 Action 仅作为**引用**存在,不含实现
- 实现位于 WASM 组件中,由 `shiroha-wasm` 按引用查找并调用
- 一个 ActionRef 至少包含:名称、版本、分发策略、签名提示

### DispatchPolicy

core 给出策略类型分类,具体路由发生在 `shiroha-dispatch`:

- **Local** — 在主控进程内执行 (本地 Executor)
- **Single** — 派发到唯一被选中的节点
- **Fanout(n)** — 派发到 n 个节点,需要聚合策略
- **Broadcast** — 派发到当前已注册的全部节点,需要聚合策略

节点选择器 (NodeSelector) 与策略组合使用:策略说"派给几个",选择器说"具体派给谁(按标签、按地域、按健康度等)"。

### Aggregation

聚合策略将多个执行结果合并为一个供 FSM 使用的输入。core 提供分类:

- **First** — 任一节点成功即返回,其余取消或忽略
- **AllOk** — 全部成功才算成功;任一失败即整体失败
- **Quorum(k)** — 至少 k 个相同(或可比)结果才返回
- **Custom** — 委托给 WASM 中用户定义的聚合函数 (见 `wit-interfaces.md`)

### Flow 与 Job

- **Flow** — 一份 FSM 定义版本,与某个 WASM 组件字节绑定
- **Job** — 一次具体运行实例,引用某个 Flow 版本,持有当前状态与累积事件

Flow/Job 是否合并为单层结构,见 `open-questions.md`。

## 不在本 crate 内的内容

为避免污染 domain 层,以下职责必须放在其他 crate:

- 任何 `async` 调用、`tokio` 类型、网络 I/O
- WASM 加载或组件实例化
- 持久化、序列化到磁盘 (但 core 类型本身需可被 `serde`/`rkyv` 序列化)
- 节点注册表、连接池

## 与其它 crate 的契约

- `shiroha-wit`:WIT 中导出的"FSM 描述符"类型必须能映射到 core 的 FSM 类型
- `shiroha-dispatch`:基于 core 的 DispatchPolicy / Aggregation 进行路由与聚合
- `shiroha-engine`:基于 core 的 FSM 模型驱动状态转移
- `shiroha-storage`:Flow / Job 的持久化结构必须能往返 core 模型

跨 crate 字段或枚举变更属于破坏性改动,需要在 PR 中说明影响范围。
