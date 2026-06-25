# Shiroha 术语表（glossary.md）

> 父任务 `.trellis/tasks/06-25-shiroha-arch` 的统一术语权威。约束 `prd.md`/`design.md`/`implement.md`/`research/` 用词一致；实现期约束代码命名（crate/struct/trait/fn 名与「代号」列对齐）与用户文档（facade API、README 用中文名）。Phase 3.3 提升到 `.trellis/spec/backend/glossary.md` 作为仓库级永久 spec；本任务归档时引用 spec 版。
>
> 每条 = 中文名 + 英文名 + 代号 + 定义 + 所属层 + 边界（「是 X，不是 Y」）+ 首次引入版本。新增术语随版本 child task 规划追加到 spec 版。

## 框架整体

| 中文名 | 英文名 | 代号 | 定义 | 所属层 | 边界 | 引入 |
|---|---|---|---|---|---|---|
| Shiroha | Shiroha | `shiroha` | 本框架名；三层 + adapter/插件体系的 Rust 框架 | 全局 | 是框架名，不是 crate 名（facade crate 也叫 `shiroha`，按上下文区分） | v0.1 |
| 编排进程 | orchestrator | `shiroha-orchestrator` | 单进程部署单元，内嵌 L1+L2+L3，与无状态 worker 通信 | 部署 | 是部署单元，不是逻辑层；单点（D6），多副本 HA 为 v0.9 进阶 | v0.8 |
| 层 | layer | L1/L2/L3 | 逻辑分层，= crate 边界（design §1） | 全局 | 是逻辑层，不是部署单元；OTel 是横切轴不是第 4 层 | v0.1 |

## L1：状态机核心 + adapter

| 中文名 | 英文名 | 代号 | 定义 | 所属层 | 边界 | 引入 |
|---|---|---|---|---|---|---|
| 状态机核心 | state machine core | `shiroha-core` | 纯引擎：层级+并行 statechart、RTC 转换、路径缓存、动作 future + 完成事件队列；不依赖 runtime/wasmtime/network | L1 | 仅依赖 `shiroha-ir`；不做 I/O，可独立单测 | v0.1 |
| 统一 IR | canonical IR | `SmIr` | 引擎唯一消费的 serde-derived 类型，位于 `shiroha-ir`；文本与 WASM CM adapter 在引擎边界前收敛于此 | L1 | 是契约不是 CM 类型；serde 派生不满足 `ComponentType`，WIT world 按其形状设计 + `From<MachineDef>` 收敛 | v0.1 |
| 状态 | state | `StateNode` | IR 中嵌套 + 正交区域的节点 | L1 | 含 children: `Vec<Region>` | v0.1 |
| 转换 | transition | `Transition` | IR 中 guard/event/source/target/action-refs 的转换定义 | L1 | 结构转换同步 RTC 原子完成 | v0.1 |
| 动作声明 | action declaration | `ActionDecl` | IR 中命名动作 → `ActionRef` 的映射 | L1 | 是声明，执行内容见 `ActionRef` | v0.1 |
| 动作引用 | action reference | `ActionRef` | 动作执行内容二选一：`WasmFunc` / `Plugin`；`Distributed` 正交包装 | L1 | 是「代码从哪来」，不是「能做什么」（后者见 Capability） | v0.1 |
| WASM 函数动作 | wasm-func action | `ActionRef::WasmFunc` | 动作在机器自身组件内（随 `define()` 打包的 wasm 的命名导出） | L1 | 是 `ActionRef` 变体，默认；不是 plugin | v0.1 |
| 插件动作 | plugin action | `ActionRef::Plugin` | 动作由 plugin 提供，`{plugin_id, method}`，wasm 或 host-native，对调用方无感知 | L1 | 是 `ActionRef` 变体；plugin_id 指 plugin 注册表条目，不含 capability 含义 | v0.1 |
| 分布式动作 | distributed action | `ActionRef::Distributed` | 正交包装：`inner`（WasmFunc/Plugin 之一）+ `fanout` + `target` + `aggregate` | L1/L2 | 是包装器，不改变 inner 执行语义 | v0.5 |
| 历史伪状态 | shallow history | `HistoryDecl` | 浅历史：记录区域最近活跃状态，重入恢复 | L1 | 深历史为可选扩展，非 MVP | v0.1 |
| Adapter | adapter | `shiroha-adapter` / `shiroha-adapter-wasm` / `shiroha-adapter-text` | 把状态机定义解释为 `SmIr` 的可插拔边界；trait crate + 实现 crate | L1 | 是定义来源解释器，不是引擎；WASM 是 MVP 首要，文本 v0.7 回归 | v0.2 |
| WASM CM adapter | WASM Component Model adapter | `shiroha-adapter-wasm` | 读 wasm 组件 `define()` → `MachineDef` → `From` → `SmIr` + 动作按名动态 `TypedFunc` 解析 | L1 | host 读结构而非运行引擎；不为每个 action 名 `bindgen!` | v0.2 |
| 文本 adapter | text adapter | `shiroha-adapter-text` | JSON/YAML/TOML → `SmIr`（serde 后端） | L1 | MVP 延后到 v0.7；`SmIr` serde-derived 保零成本回归 | v0.7 |
| 机器定义 | machine definition (CM) | `MachineDef` | `bindgen!` 生成的 typed CM record，`define()` 返回 | L1 | 是 CM 侧类型，经 `From<MachineDef> for SmIr` 收敛；不是引擎消费类型 | v0.2 |

## L1：动作执行模型

| 中文名 | 英文名 | 代号 | 定义 | 所属层 | 边界 | 引入 |
|---|---|---|---|---|---|---|
| 结构转换步 | structural transition step | — | 选转换、算 LCA、确定 exit/run/enter 顺序，同步 RTC 原子完成 | L1 | 不做中途突变；动作异步见下 | v0.1 |
| 异步动作 | async action | — | 转换结构确定后触发的 entry/run/exit action，`async`，引擎不阻塞等待 | L1 | 完成抛 `done.<action>`/`error.<action>` 事件回流驱动后续转换（xstate-invoke 风格） | v0.1 |
| 完成事件 | completion event | `done.*` / `error.*` | 动作完成（成功/失败）抛回实例事件队列的事件 | L1 | 是事件不是返回值；引擎管理 in-flight 动作与完成事件队列 | v0.1 |

## Capability（权限轴，正交于 plugin）

| 中文名 | 英文名 | 代号 | 定义 | 所属层 | 边界 | 引入 |
|---|---|---|---|---|---|---|
| 能力 | capability | `CapabilityDecl` | wasm 组件 `import` 的所有 host 提供物，运行时权限范围；`SmIr` 内声明 | L1（横切） | 是「wasm 能做什么」，与 plugin 无交集；不是 action 代码来源 | v0.1（契约）/ v0.10（完整授权） |
| WASI 能力 | WASI capability | `wasi:*` | 标准 WASI worlds：`wasi:io`/`wasi:clocks`/`wasi:filesystem`/`wasi:sockets`/`wasi:http` | L1（横切） | 是 capability 子集；`http`/`fs` 属此，不是 plugin | v0.10 |
| 框架原生能力 | framework-native capability | `shiroha:*` | WASI 表达不了的能力，框架 host-native 实现的 interface：`shiroha:shell`/`shiroha:log` | L1（横切） | 是 capability 子集；与 WASI world 并列于 `CapabilityDecl`；不是 plugin | v0.10 |
| 能力授权 | capability authorization | — | task 创建时声明所需 caps → 申请 → host 白名单注入 wasmtime Linker → 未授权拒绝实例化 | L3（流程） | 是运行时授权流程，v0.10 完整化；MVP 用最小 host-func 通道直接接线 | v0.10 |
| 能力白名单 | capability whitelist | — | host 按 task 授权注入的 cap 子集；未授权 import 槽位不注册 → 实例化失败 = 自然能力协商 | L1/L3 | 是注入策略，不是声明 | v0.10 |
| 能力合集 | capability union | — | task 授权 = 机器组件 caps ∪ 所调用 wasm plugin caps | L3 | 是授权计算口径，用于多 plugin 调用场景 | v0.10 |

## Plugin（扩展轴，正交于 capability）

| 中文名 | 英文名 | 代号 | 定义 | 所属层 | 边界 | 引入 |
|---|---|---|---|---|---|---|
| 插件 | plugin | — | action/聚合的扩展机制，`ActionRef::Plugin`/`AggregateRef::Plugin` 调用 | L1（横切） | 是「action 代码从哪来」的扩展，与 capability 无交集 | v0.4 |
| WASM 插件 | wasm plugin | — | plugin 的一种实现：wasm 组件，受声明的 WASI/`shiroha:*` caps 约束 + 沙箱 | L1（横切） | 是 plugin 子集；运行时按 caps 授权 | v0.4 |
| 宿主原生插件 | host-native plugin | — | plugin 的一种实现：Rust crate 部署期链接，用于 WASI 表达不了的功能 | L1（横切） | 是 plugin 子集；信任由部署期建立（签名/配置白名单），不经运行时 cap 授权；不是 capability | v0.4 |
| 插件注册表 | plugin registry | — | plugin_id → plugin 实现的注册表，semver major 协商 | L1/L3 | 是 plugin 解析中介，不是 capability 注册表 | v0.4 |
| 聚合器 | aggregator | `AggregateRef` | 分布式动作结果聚合策略；`Builtin`（Rust 原生）/ `WasmFunc` / `Plugin` | L1/L2 | 是聚合策略，不是 action；自定义走 CM resource 句柄 | v0.5 |
| 内置聚合 | builtin aggregate | `AggregateRef::Builtin` | Rust 原生零开销聚合：`All`/`Any`/`Quorum(n)`/`FirstSuccess` | L2 | 是 `AggregateRef` 变体 | v0.5 |

## L2：分布调度器

| 中文名 | 英文名 | 代号 | 定义 | 所属层 | 边界 | 引入 |
|---|---|---|---|---|---|---|
| 分布调度器 | scheduler | `shiroha-scheduler` | 把 `distributed` action fan-out 到无状态 worker，聚合结果 | L2 | 不做工作流引擎；跨动作编排由状态机结构建模 | v0.5 |
| 分发单元 | dispatch unit | — | 标注 `distributed` 的 action | L2 | 是 action，不是 task | v0.5 |
| 分发扇出 | fan-out | `fanout` | 分发数量：`None`=单点 1 worker，`N`=N 片 | L2 | 是数量控制，与 target 正交 | v0.5 |
| 分发目标 | dispatch target | `TargetSpec` | 节点约束：`Any`/`Pool`/`Label`/`Explicit` | L2 | 是节点偏好，与 `required_capabilities` 两层过滤 | v0.5 |
| Worker | worker | `shiroha-worker` | 无状态动作执行器，收到 `ActionDispatch` 执行后回 `ActionResult` | L2 | 无跨调用状态；不是编排进程 | v0.5 |
| 传输 | transport | `shiroha-transport` / `shiroha-transport-grpc` | 抽象 `Transport` trait + 默认 tonic gRPC bidi 实现 | L2 | 是 trait + 实现；可换 libp2p/QUIC | v0.5 |
| 分发请求 | action dispatch | `ActionDispatch` | transport-domain 纯 Rust struct，编排→worker | L2 | 不是 prost 类型；grpc 实现边界做映射 | v0.5 |
| 动作结果 | action result | `ActionResult` | worker→编排的结果，`done`/`error` | L2 | proto 与聚合策略无关；聚合全在编排侧 | v0.5 |
| 分发标识 | dispatch id | `dispatch_id` | 分布式动作分发的唯一标识，用于崩溃恢复关联 | L2 | v0.5 预留字段，v0.9 完整化关联 | v0.5 |

## L3：控制器 + 可观测性 + 持久化

| 中文名 | 英文名 | 代号 | 定义 | 所属层 | 边界 | 引入 |
|---|---|---|---|---|---|---|
| 控制器 | controller | `shiroha-controller` | L3：task CRUD/pause/resume/query、多实例托管、auth、能力校验、内嵌 L1+L2 | L3 | 是嵌入编排进程的库，不是独立服务；非框架功能由宿主自实现 | v0.3 |
| 任务 | task | `TaskId` | 一个状态机实例；多实例并发托管，每实例独立事件队列 | L3 | task = 实例，不是状态机定义 | v0.3 |
| 多实例托管 | multi-instance hosting | — | 每 task 一个 `Store<T>`（wasm）+ 独立事件队列 + root `tracing::Span` | L3 | `Store` `!Sync` → `LocalSet` 或 `Arc<Mutex<Store>>` | v0.3 |
| 持久化策略 | persistence policy | `PersistencePolicy` | per-task 可选：`None`/`Realtime`/`Deferred`，创建 task 时声明 | L3 | 是 per-task 可选，不默认强加落盘开销 | v0.3 |
| 实时持久化 | realtime persistence | `PersistencePolicy::Realtime` | 每事件/转换决策/完成事件同步追加落盘，可配 fsync，崩溃零丢失 | L3 | 安全/可恢复优先；性能开销大 | v0.3 |
| 延迟持久化 | deferred persistence | `PersistencePolicy::Deferred` | 按时间窗口/事件数批量落盘或仅 snapshot，崩溃丢窗口内进度 | L3 | 性能敏感；恢复 = 最近 snapshot + 之后部分 log | v0.3 |
| 无持久化 | no persistence | `PersistencePolicy::None` | 纯内存，最快，不可恢复（默认） | L3 | 是默认，不是禁用 | v0.3 |
| 事件存储 | event store | `EventStore` trait | append-only log + snapshot read/write 抽象，默认本地文件实现 | L3 | 是存储后端抽象，进阶可换分布式存储（v0.9） | v0.3 |
| 事件溯源 | event sourcing | — | 事件流（submitted events + transition decisions + completion events）追加落盘，崩溃后重放重建内存 active state | L3 | 是主路径恢复机制；snapshot 加速重放 | v0.3 |
| 快照 | snapshot | — | 周期性把 active state configuration（当前状态集 + in-flight 动作表 + 事件队列）落盘 | L3 | 是重放加速，不是主路径 | v0.3 |
| 可观测性 | observability | `shiroha-otel` | 横切轴：`tracing` 埋点织入所有层 + OTel SDK/exporter 装配隔离单 crate | 横切 | **不是第 4 个功能层**；由 L3 控制器 wiring | v0.3（基础）/ v0.6（完整） |
| 任务 span | per-task span | — | 每 task 一个 root `tracing::Span`，属性 `task_id`+`machine_name` | 横切 | 是 trace 单元，跨 worker 传播 | v0.6 |

## 安全

| 中文名 | 英文名 | 代号 | 定义 | 所属层 | 边界 | 引入 |
|---|---|---|---|---|---|---|
| 认证 | authentication | — | 控制器 API token/api-key；worker 共享 token，TLS 下可升 mTLS | L3 | 是身份校验，不是授权（后者见 capability authorization） | v0.8 |
| 传输加密 | transport encryption | — | `shiroha-transport-grpc` rustls features 按需开启 | L2 | 是链路加密，不是身份认证 | v0.8 |
| 沙箱 | sandbox | — | wasm 动作/聚合器/plugin 受 fuel + epoch + StoreLimits + timeout 约束 | L1/L2 | 是 wasm 运行时约束，不是 host-native plugin（后者部署期信任） | v0.2 |
| Facade | facade | `shiroha` | 电池齐全默认栈 crate，feature flags 可关单项瘦身 | L3 | 是组合入口，不是逻辑层 | v0.8 |

## 版本里程碑（代号，非术语）

> 以下为路线图版本代号，不是框架抽象术语，仅用于 child task 规划引用。详见 `implement.md`。

| 版本 | 交付物 | 引入的新术语 |
|---|---|---|
| v0.1 | 引擎内核 | `SmIr`/`StateNode`/`Transition`/`ActionDecl`/`ActionRef`/`HistoryDecl`/`CapabilityDecl`(契约) |
| v0.2 | WASM 单机运行 | `shiroha-adapter-wasm`/`MachineDef`/沙箱 |
| v0.3 | 控制器+多实例+持久化 | `shiroha-controller`/`TaskId`/`PersistencePolicy`/`EventStore`/event sourcing/snapshot |
| v0.4 | plugin 完整化 | plugin/wasm plugin/host-native plugin/`AggregateRef` 接口 |
| v0.5 | 分布调度+worker | `shiroha-scheduler`/`shiroha-transport`/`shiroha-worker`/`ActionDispatch`/`ActionResult`/`dispatch_id`/`AggregateRef::Builtin` |
| v0.6 | OpenTelemetry | `shiroha-otel`/per-task span |
| v0.7 | 文本 adapter | `shiroha-adapter-text` |
| v0.8 | 安全+facade+生产化 | `shiroha-orchestrator`/`shiroha`(facade)/认证/传输加密 |
| v0.9 | 持久化增强+多副本 HA | （增强现有，无新核心术语） |
| v0.10 | 完整 capability 授权 | WASI capability/框架原生 capability/能力授权/能力白名单/能力合集 |

## 维护规则

1. **planning 期**：本文件是 prd/design/implement/research 用词权威；发现未收录术语或用词不一致，先追加/修正本文件，再回改其他产物。
2. **实现期（Phase 3.3）**：提升到 `.trellis/spec/backend/glossary.md`，成为仓库级永久 spec；任务目录版在归档时引用 spec 版。
3. **演进**：每个版本 child task 规划时，新增术语追加到 spec 版 glossary 的对应表；代号列与最终代码命名（crate/struct/trait/fn）保持对齐，若实现期发现代号不可行，回改本文件再改代码。
4. **约束力**：planning 约束产物用词；spec 版约束代码命名 + 用户文档（facade API、README 用中文名）。
