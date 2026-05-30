# 持久化层 (shiroha-storage)

## 角色

主控持有 Flow / Job / Event 的运行时状态;这些状态必须在重启后能完整恢复。`shiroha-storage` 定义统一的 Store trait,并提供基于 redb 的默认实现。

节点端无状态,**不依赖本层**。

## 存储模型

本系统采用**状态快照 + 审计日志**的简化模型,而非纯事件溯源。Job 持有独立的当前状态字段(每次转移时更新),Event 作为不可变的附加日志流用于审计、调试与观测。状态恢复依赖 Job 快照而非从 Event 重建。

## Store 抽象

Store 至少覆盖四类记录:

| 记录 | 内容 | 写入时机 |
| --- | --- | --- |
| Component | 一份 WASM 字节,以内容 hash (`ComponentId`) 为主键;可被多个 Flow 版本引用 | 上传时去重写入,后续只读 |
| Flow | FSM 定义版本(name + version + 能力声明)+ 引用一个 ComponentId | 上传时一次写入,后续只读 |
| Job | 一次 FSM 运行实例的当前状态 + 元数据 + 引用的 ComponentId | 每次状态转移更新 |
| Event | 状态转移、Action 派发、结果聚合的不可变事件流 | append-only |
| DeadLetter | Aggregator 提前返回后晚到的结果,或节点离线期间丢失的结果 | Dispatch 层写入 |

## Flow 与 ComponentId

Flow 是 FSM 定义版本的**人类语义包装**:一个 Flow 包含 name、version、能力声明,并引用一个 ComponentId。version 由主控在 upload-flow 时自动递增(同名 Flow 的 version = 已有最大 version + 1),用户无需手动指定。多个 Flow(同名不同版本,或不同名)可引用同一 ComponentId(例如"只改版本号不改实现"的情形)。create-job 时可选指定 `(name, version)` 或默认使用最新版本。

ComponentId 由 Component 字节的内容 hash 派生,主控用它做去重存储。所有跨主从边界的引用都使用 ComponentId,因此 `shiroha-core` 不引入 Flow 类型——Flow 只对主控自己与控制面用户有意义。

删除 Flow 不会自动删除 Component 字节;Component 仅在没有任何 Flow 或 Job 引用时才能被清理(见下方"Component 生命周期")。

## 版本演进

ComponentId 与 Flow 的拆分让版本演进不需要改动 `shiroha-core` 或 worker。

1. 用户上传同名 Flow 的新字节 → 主控派生新 `ComponentId`(内容不同,hash 必然不同)
2. 同事务内:若 ComponentId 未命中去重则写入新 Component 字节;追加一条新 Flow 记录 `(name, version+1, new_component_id, 能力声明)`
3. 旧 Flow 记录与旧 Component 字节均保留,被旧 Job 的引用计数保护
4. 新建 Job 默认指向"最新版本",也可在 `create-job` 时显式指定 version
5. 进行中的 Job **不受影响**,继续在旧 ComponentId 上运行至终态

这条路径的全部"版本智能"集中在 storage + engine + control,worker 与 transport 完全无感。

需要"修了 bug 让在跑的 Job 也用上"的场景:**当前版本不支持跨版本迁移**——建议取消旧 Job、用新版本重建。跨版本迁移 + 状态映射列入后续版本路标。

### Component 字节的生命周期

- 引用计数 = (引用该 ComponentId 的 Flow 数量)+(引用该 ComponentId 的 Job 数量,含非终态与终态)
- 计数归零后先标记为"待清理",延迟 N 分钟(可配置,默认 10 分钟)后二次确认无新引用再删除字节
- 这一延迟清除策略避免并发事务同时释放最后一个引用时的计数竞态,以及 GC 进程在删除字节后、更新计数前崩溃导致的不一致
- 强制清理终态 Job 的命令应先取消其对 Component 的引用,GC 由后台周期任务处理

## 原子性约束

任何状态转移都必须满足:Job 当前状态的写入与对应 Event 的 append 是**同一个事务**。半成品写入会导致重启后 Job 实际状态与事件流不一致。

约束的直接体现:Store 必须提供等价于 `save_job_with_event` 的复合写入接口,而非两次独立写入。所有 Engine 内的状态变更都走这条接口,不允许"先写状态再写事件"或反之。

## 失败与回滚

- **Job 创建失败** — 已写入的 Job 记录必须被显式删除(回滚),不能留下"无主"记录
- **初始 Action 硬失败** — 同上,清理已持久化的 Job 与所有相关 Event
- **中途转移失败** — 不允许部分写入;事务在 Store 实现层面回滚
- **Flow 上传中途失败** — 字节与元数据必须一并写入或一并丢弃

## 版本与历史

- 同一 Flow 的多个版本独立保留;Job 引用具体 ComponentId 而非"最新指针"
- 删除 Flow 的处置:**默认拒绝删除**(只要还存在引用此 Flow 的非终态 Job);加 `--force` 时先取消所有相关非终态 Job 再删 Flow;终态 Job 是否级联清理由 delete-flow 命令的标志位决定。具体语义在 Engine,不在 storage 层决策
- Event 保留:**与 Job 生命绑定**——Job 终态后 N 天(配置项,默认 30 天)清理其全部事件;运行中 Job 的事件始终保留。如需长期分析,消费方应在 Job 终态前主动 `tail-events` 落到外部系统
- **Event 导出** — `sctl` 提供 `export-events <job-id>` 命令,在清理前将事件导出为 JSON Lines 格式;也可通过控制面 gRPC 的 `ExportEvents` RPC 程序化调用

## redb 实现备注

选用 redb 的理由:

- 单文件、嵌入式,匹配 KISS
- 原生事务,直接服务于上述原子写约束
- 纯 Rust,不引入 C 依赖,跨平台编译友好

替换为其他后端 (sled / sqlite / lmdb) 时,Store trait 是唯一接触面;Engine 不感知。新增后端可作为独立 crate,通过 Cargo feature 选择。

## 查询接口

除了写入,Store 也提供只读查询,供控制面观测命令使用:

- 按 Id 取 Flow / Job
- 列出 Flow / Job(支持过滤与游标)
- 按 Job 拉取 Event 流(游标-based:游标可重放、保证单调,客户端断流后可续上)

游标为 **Event ID**(单调递增的 64 位整数,由 Store 在 append 时分配)。客户端传入上次收到的 Event ID,Store 返回该 ID 之后的事件流。游标语义在 Store trait 中定义,具体实现保证单调与可重放。

## 与其他 crate 的契约

- 入参:`shiroha-core` 的 Job / Event 类型;Flow / Component 由 storage 自定义(不在 core)
- 调用方:`shiroha-engine` 与 `shiroha-control`(后者仅做只读查询,如 list-flows、tail-events)
- 不调用:不调用任何 wasm / transport / dispatch 接口
