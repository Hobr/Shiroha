# 持久化层 (shiroha-storage)

## 角色

主控持有 Flow / Job / Event 的运行时状态;这些状态必须在重启后能完整恢复。`shiroha-storage` 定义统一的 Store trait,并提供基于 redb 的默认实现。

节点端无状态,**不依赖本层**。

## Store 抽象

Store 至少覆盖三类记录:

| 记录 | 内容 | 写入时机 |
|---|---|---|
| Flow | FSM 定义版本 + 关联 WASM 组件字节 + 能力声明 | 上传时一次写入,后续只读 |
| Job | 一次 FSM 运行实例的当前状态 + 元数据 | 每次状态转移更新 |
| Event | 状态转移、Action 派发、结果聚合的不可变事件流 | append-only |

## 原子性约束

任何状态转移都必须满足:Job 当前状态的写入与对应 Event 的 append 是**同一个事务**。半成品写入会导致重启后 Job 实际状态与事件流不一致,从而破坏事件溯源能力。

约束的直接体现:Store 必须提供等价于 `save_job_with_event` 的复合写入接口,而非两次独立写入。所有 Engine 内的状态变更都走这条接口,不允许"先写状态再写事件"或反之。

## 失败与回滚

- **Job 创建失败** — 已写入的 Job 记录必须被显式删除(回滚),不能留下"无主"记录
- **初始 Action 硬失败** — 同上,清理已持久化的 Job 与所有相关 Event
- **中途转移失败** — 不允许部分写入;事务在 Store 实现层面回滚
- **Flow 上传中途失败** — 字节与元数据必须一并写入或一并丢弃

## 版本与历史

- 同一 Flow 的多个版本独立保留;Job 引用具体版本号而非"最新指针"
- 删除 Flow 默认级联删除其所有版本与所属终态 Job;非终态 Job 的处置由 Engine 决定(拒绝删除 / 强制清理),不在 storage 层决策
- Event 流保留期默认无限;清理策略(TTL / 容量)待定,见 `open-questions.md`

## redb 实现备注

选用 redb 的理由:

- 单文件、嵌入式,匹配 KISS
- 原生事务,直接服务于上述原子写约束
- 纯 Rust,不引入 C 依赖,跨平台编译友好

替换为其他后端 (sled / sqlite / lmdb) 时,Store trait 是唯一接触面;Engine 不感知。新增后端可作为独立 crate,通过 Cargo feature 选择。

## 与其他 crate 的契约

- 入参:`shiroha-core` 的 Flow / Job / Event 类型
- 调用方:`shiroha-engine` 与 `shiroha-control`(后者仅做只读查询,如 list-flows、tail-events)
- 不调用:不调用任何 wasm / transport / dispatch 接口

## 查询接口

除了写入,Store 也提供只读查询,供控制面观测命令使用:

- 按 Id 取 Flow / Job
- 列出 Flow / Job(支持过滤与游标)
- 按 Job 拉取 Event 流(游标-based,见历史实现的"cursor-based polling"语义)

游标语义在 Store trait 中给出,具体实现保证单调与可重放。
