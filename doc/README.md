# Shiroha 设计文档

Shiroha 是一款基于 WebAssembly 组件模型的有限状态机编排框架。本目录记录其整体框架、各子系统的职责边界,以及跨模块契约。

## 阅读顺序

1. [`architecture.md`](architecture.md) — 顶层架构与角色分工
2. [`workspace.md`](workspace.md) — Workspace 布局与 crate 依赖方向
3. [`core-model.md`](core-model.md) — 状态机 domain 模型与策略类型
4. [`wit-interfaces.md`](wit-interfaces.md) — WASM 接口与能力契约
5. [`dispatch.md`](dispatch.md) — Action 分发与结果聚合
6. [`transport.md`](transport.md) — 节点间传输层抽象
7. [`storage.md`](storage.md) — 主控持久化层
8. [`engine.md`](engine.md) — 主控引擎与 Job 生命周期
9. [`worker.md`](worker.md) — 节点端执行器
10. [`control-plane.md`](control-plane.md) — sctl 与 shirohad 的控制面
11. [`data-flow.md`](data-flow.md) — 端到端数据流
12. [`open-questions.md`](open-questions.md) — 待对齐的设计决策

## 写作原则

- 描述「是什么」与「为什么」,不写实现代码
- 跨 crate 的契约在双方文档中同时给出指向
- 任何尚未对齐的取舍统一归档到 `open-questions.md`,避免散落在各处
- 设计决策一旦敲定,从 `open-questions.md` 迁移到对应模块文档并删除原条目

## 术语约定

| 词 | 含义 |
|---|---|
| FSM | Finite-State Machine,有限状态机 |
| Flow | 一份 FSM 定义版本(name + version + 能力声明);**主控层独有概念**,引用一个 ComponentId |
| ComponentId | WASM 组件字节的内容 hash;跨主从边界的不透明标识 |
| Job | 一次具体的 FSM 运行实例,引用一个 ComponentId |
| Action | FSM 中需要被执行的计算单元;可被分发到本地或远端 |
| WaitingMode | ActionRef 上的声明,决定派发期间 Job 是阻塞还是显式进入 Waiting |
| 主控 / Master | `shirohad` 以主控模式运行的实例 |
| 节点 / Worker | `shirohad` 以节点模式运行的实例,执行被分发的 Action |
| 控制面 | sctl 与主控之间的接口面 |
| 节点面 | 主控与节点之间的接口面 |
