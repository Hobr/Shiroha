# Shiroha 框架整体架构规划

## Goal

设计并确立 **Shiroha**——一个用 Rust 开发的三层框架的整体架构。本任务为**父任务**，统一管理需求集、三层边界与契约、跨层验收标准与技术选型评估；各层具体实现由后续 child task 承担，父任务做最终集成评审。

## Background

Shiroha 由三层 + 一套 adapter / 框架插件体系组成：

1. **第一层：状态机核心 + adapter**
   - 状态机要求高性能 + 功能强大；通过不同 **解释 adapter** 接入状态机定义：文本 adapter（JSON/YAML/TOML）与 WASM Component Model adapter。
   - WASM CM adapter：组件把「状态机定义 + 动作实现」打包在一起；Rust 宿主是引擎，加载组件后读取 `define()` 导出的结构构建 IR，状态机引擎在 Rust 侧运行；触发动作时宿主按名调用组件导出函数；动作函数可 import host func 获取能力（= 框架插件通道）。
   - `action` / `callback` 执行内容二选一：`wasm func`（动作在机器自身组件内，默认）或 `plugin`（插件调用）；`http`/`shell`/`fs` 等是**框架内置插件**（用 wasm + WASI/host func 实现），文本/wasm 定义可直接调用，无需写代码。
2. **第二层：分布调度器**——把被指定分发的 action 发到无状态节点执行，返回聚合结果。
3. **第三层：控制器**——可被 GUI/CLI/Web 引入，做任务管理 + OpenTelemetry + 简单安全校验；非框架功能由宿主自实现。

## Key Decisions（产品决策溯源，9 项均已确定）

| # | 决策点 | 结论 |
| --- | --- | --- |
| D1 | 状态机语义模型 | 层级 + 并行状态图（UML/SCXML 风格）：嵌套状态 + 正交/并行区域 + guard + entry/exit/run action + 浅历史；扁平化内部表示 + 转换路径缓存保性能 |
| D2 | WASM CM adapter 暴露方式 | typed data 定义（`define() -> StateMachineDef`）+ 每动作命名导出；host 读定义建 IR、预链接动作引用、按名调用 |
| D3 | 动作执行语义 | 异步动作 + 完成事件（xstate-invoke 风格）；结构转换同步 RTC 原子完成，动作异步，完成抛 `done.*`/`error.*` 回流 |
| D4 | 分布分发单元与聚合 | action 为单元 + fan-out/fan-in；节点为无状态动作执行器；聚合策略 all/any/quorum(n)/first-success |
| D5 | 实例/任务模型 | 多实例引擎，task = 一个状态机实例；状态默认驻内存；持久化与崩溃恢复为可选能力 |
| D6 | 部署拓扑 | 单编排进程（内嵌 L1+L2+L3）+ 无状态 worker |
| D7 | 能力/插件分离模型 | **capability 与 plugin 正交、无交集**：capability = wasm 运行时权限范围（WASI worlds `wasi:io`/`clocks`/`filesystem`/`sockets`/`http` + 框架原生 `shiroha:*` interface，如 `shiroha:shell`/`shiroha:log`），wasm 组件与 wasm plugin 均声明所需 caps，task 创建时申请授权、host 按白名单注入 wasmtime Linker；plugin = action/聚合的扩展机制（`ActionRef::Plugin{plugin_id, method}` / `AggregateRef::Plugin{...}`），可 wasm 或 host-native，对调用方无感知。`http`/`fs` 等是 WASI/框架 caps 而非 plugin；`shell`/`log` 是框架原生 caps。semver major 协商用于 plugin；沙箱=fuel/epoch/StoreLimits/timeout |
| D8 | 安全校验 MVP | token/api-key 认证 + 动作能力校验 + TLS 可选；RBAC/多租户为进阶 |
| D9 | 传输层 | 抽象 `Transport` trait + 默认 tonic(gRPC) 实现；双向流承载分发与回流 |

## Requirements

### R1 状态机核心
- R1.1 高性能（具体指标在 child task 定：per-event 处理开销目标接近扁平 FSM）。
- R1.2 语义模型见 D1：嵌套状态 + LCA 转换链、正交/并行区域、guard、entry/exit/run action、浅历史（深历史为可选扩展）。
- R1.3 状态机定义来源可插拔，由 adapter 解释为统一 IR 供核心引擎消费。
- R1.4 动作执行语义见 D3：结构转换同步 RTC；动作异步；完成抛 `done.<action>`/`error.<action>` 事件回流；引擎管理 in-flight 动作与完成事件队列。

### R2 Adapter 体系
- R2.1 文本 adapter（JSON/YAML/TOML）：**MVP 延后**，专注 WASM 作为首要定义来源；`SmIr` 仍保持 serde-derived 以便后期文本 adapter 零成本回归（v0.7）。
- R2.2 WASM Component Model adapter见 D2：组件导出 `define() -> StateMachineDef`（CM typed record）+ 每动作命名导出；host 读定义建 IR 并预链接动作引用，按名调用；动作可 import host func（插件通道）。
- R2.3 adapter 产出统一 IR；动作引用统一为 `{kind, ref}`，**执行内容二选一**：
  - `{wasm-func, <export>}`（默认）——动作在**机器自身组件**内（随 `define()` 打包的那份 wasm）。
  - `{plugin, <cap>.<method>}`——动作由**插件**提供；`http`/`shell`/`fs` 等是**框架内置插件**（用插件机制实现的示例），用户可加自定义插件。
  - `{distributed, inner, fanout, target, aggregate}`——正交包装，`inner` 为上面二选一。

### R3 Action / Callback 类型与 Plugin 扩展
- R3.1 动作执行内容**二选一**：`wasm func`（机器自身组件命名导出，默认）或 `plugin`（plugin 调用，`ActionRef::Plugin{plugin_id, method}`）。
- R3.2 plugin 是 action/聚合的**扩展机制**，可 wasm 或 host-native，对调用方无感知（用户只写 `{plugin_id, method}`，不关心背后实现与所用 caps）。**plugin 不是 capability**：`http`/`fs` 等不再伪装成 plugin，而是 WASI/框架 caps（见 R3.5）。
- R3.3 框架自带可选 host-native plugin（如 `shell`/`log` 的 action 包装），用户可加自定义 plugin（wasm 或 host-native）。wasm plugin 受其声明的 WASI/框架 caps 约束；host-native plugin 信任由部署期建立（签名/配置白名单）。
- R3.4 plugin semver major 协商 + 注册表；沙箱（fuel/epoch/StoreLimits/timeout）适用于 wasm plugin。

### R3.5 Capability（权限轴，正交于 plugin）
- R3.5.1 capability = wasm 组件 `import` 的所有 host 提供物，统一为两类：**WASI 标准 worlds**（`wasi:io`/`wasi:clocks`/`wasi:filesystem`/`wasi:sockets`/`wasi:http`）+ **框架原生 interface**（`shiroha:shell`/`shiroha:log` 等 WASI 表达不了的）。
- R3.5.2 wasm 组件声明所需 caps；task 创建时申请授权（声明→申请→白名单注入→未授权拒绝实例化）；wasm plugin 同样声明所需 caps，task 授权合集 = 机器组件 caps ∪ 所调用 wasm plugin caps。
- R3.5.3 host 用 wasmtime `Linker` 按 task 授权白名单注入；未声明/未授权槽位不注册 → 实例化失败 = 自然能力协商。
- R3.5.4 **完整 capability + task 创建授权为未来版本目标（v0.10，非 MVP）**。MVP（v0.2/v0.4）保留最小 host-func 通道直接接线让示例能跑；IR 契约（`SmIr` 的 `CapabilityDecl`）在 v0.1 一次定对，避免破坏 G2 冻结点。

### R4 分布调度器
- R4.1 分发单元 = 标注 `distributed` 的 action（可带 fan-out 分片 + 目标约束）。
- R4.2 节点为无状态动作执行器。
- R4.3 结果以 `done.<action>`/`error.<action>` 事件回流。
- R4.4 聚合策略：**内置策略用 Rust 原生实现（零开销）**（all/any/quorum(n)/first-success）；**自定义策略走 wasm 插件**（有状态聚合器，CM resource 句柄：create/on-result/destroy），受沙箱约束，复用框架插件通道。IR `Aggregate` 改为 `{builtin, ...} | {wasm, ref}` 引用，不再固定 enum。
- R4.5 调度器不做工作流引擎；跨动作编排由状态机结构建模。
- R4.6 节点发现 / 重试 / 一致性策略在 child task 细化（传输见 D9）。
- R4.7 分发目标控制：`fanout: N` 控数量（不声明=单点分发 1 worker）；`target: {any | pool:<name> | label:<k=v> | explicit:[endpoint...]}` 控节点约束（默认 `any` 让调度器自主负载均衡，`required_capabilities` + `target` 两层过滤）。

### R5 控制器
- R5.1 可被 GUI/CLI/Web 引入。
- R5.2 任务管理：task = 状态机实例（见 D5）；多实例并发托管，每实例独立事件队列；CRUD/暂停/恢复/查询。
- R5.3 持久化与崩溃恢复：**per-task 可选**，三种模式——`Realtime`（实时，每事件同步落盘，安全/可恢复优先）、`Deferred`（延迟，按时间窗口/事件数批量落盘或仅 snapshot，性能敏感）、`None`（纯内存，最快不可恢复，默认）。控制器按 task 声明。详见 R7。
- R5.4 集成 OpenTelemetry：trace 优先（per-task span，跨 worker 传播），metrics 次之，logs 经 appender。
- R5.5 安全校验 MVP 见 D8。
- R5.6 非框架功能由宿主自实现。
- R5.7 嵌入形态（由 D6 派生）：控制器在编排进程内，暴露统一 `Controller` API；GUI/CLI 经进程内库 facade 或本地 IPC/HTTP；Web 经 service boundary。编排进程单点靠「持久化+多副本 HA」缓解（进阶，非 MVP）。

### R6 技术选型
- R6.1 WASM 运行时：`wasmtime` 46.x（features `component-model`+`component-model-async`+`async`+`cranelift`+`pooling-allocator`+`cache`）。详见 research/01。
- R6.2 异步运行时：`tokio`（`rt-multi-thread`+`macros`+`time`+`signal`）。详见 research/02。
- R6.3 序列化：`serde`+`serde_json`+`serde-saphyr`(YAML)+`toml`；统一 IR 为 `SmIr`。详见 research/03。
- R6.4 传输：`tonic` 0.14 + `prost` 0.14 默认 gRPC + 抽象 `Transport` trait。详见 research/04。
- R6.5 可观测性：`tracing` 0.1 + `opentelemetry` 0.32 family + `tracing-opentelemetry` 0.33 + `opentelemetry-appender-tracing`，隔离于 `shiroha-otel`。详见 research/05。
- R6.6 工作区：12-crate Cargo workspace，`shiroha-ir` 为 serde-only 叶子，`shiroha-core` 仅依赖 ir，wasmtime≤2 crate、tonic≤2、OTel=1。详见 research/06。
- R6.7 部署拓扑见 D6。

## Acceptance Criteria

- [ ] AC1 三层边界与职责划分明确，每层对外契约定义完成。
- [ ] AC2 adapter↔core 的统一 IR（`SmIr`）契约定义完成（serde-derived，文本与 CM 两路收敛）。
- [ ] AC3 WASM CM adapter「读结构而非运行引擎」语义精确定义：`define()`→IR + 命名导出按名动态调用 + host-func 插件通道。
- [ ] AC4 **capability/plugin 分离模型**：capability（WASI worlds + 框架原生 `shiroha:*` interface，task 创建授权）与 plugin（action/聚合扩展，`{plugin_id, method}`，wasm 或 host-native）正交无交集，IR `CapabilityDecl` + `ActionRef::Plugin` 契约定义完成；MVP 执行边界（v0.2/v0.4 最小 host-func 通道）与 v0.10 完整授权 feature 划分明确。
- [ ] AC5 分布调度器分发单元（distributed action）、聚合策略、节点无状态假设、失败回流定义完成。
- [ ] AC6 控制器嵌入形态、任务管理、OpenTelemetry 集成（per-task span + 跨 worker 传播）、安全校验范围定义完成。
- [ ] AC7 关键技术选型（R6.1–R6.6）有评估结论与选定理由（见 research/）。
- [ ] AC8 拆分出可独立交付的 child task 列表，并标注依赖顺序（见 implement.md）。
- [ ] AC9 统一术语表（`glossary.md`）建立：框架各抽象有中文名 + 英文名 + 代号 + 定义 + 所属层 + 边界 + 引入版本；prd/design/implement/research 用词与之一致；Phase 3.3 提升为仓库级 spec。

## Out of Scope

- 各层具体代码实现（属于 child task）。
- GUI / CLI / Web 端自身业务功能。
- 业务领域状态机示例库。
- RBAC / 多租户隔离 / 多副本 HA（进阶，非 MVP）。
- 文本 adapter（JSON/YAML/TOML）延后到 v0.7，非 MVP 首要。
- 完整 WASI/框架 caps + task 创建授权（v0.10），非 MVP。

## Version Roadmap（多版本分步实现）

详细见 `implement.md`。每个版本 = 一个 child task，**到点才创建并细规划**（不一次性建全部 child），让下一版本可基于上一版本测试结果微调。MVP 聚焦 WASM 路径。

| 版本 | 交付物 | 依赖 |
| --- | --- | --- |
| v0.1 | 引擎内核（ir+core，纯逻辑，mock 动作）；`SmIr` 含 `CapabilityDecl` + `ActionRef::Plugin{plugin_id, method}` 一次定对 | — |
| v0.2 | WASM adapter + 最小 host-func 通道（直接接线，无完整授权）+ runner | v0.1 |
| v0.3 | 控制器+多实例+持久化 | v0.1, v0.2 |
| v0.4 | plugin 完整化（wasm plugin 加载 + host-native plugin 注册表 + semver + 资源限额沙箱；capability 授权从本版剥离至 v0.10）+ 聚合器接口 | v0.2 |
| v0.5 | 分布调度器 + 无状态 worker（transport+tonic+scheduler+wasm 聚合器） | v0.2, v0.4 |
| v0.6 | OpenTelemetry（OTel export + 跨 worker trace 传播） | v0.3, v0.5 |
| v0.7 | 文本 adapter 回归（JSON/YAML/TOML） | v0.1 |
| v0.8 | 安全 + facade + 生产化（auth+TLS+编排 bin+Web boundary） | v0.3–v0.7 |
| v0.9 | 持久化增强 + 多副本 HA（进阶，基于 v0.3 基础） | v0.5, v0.8 |
| v0.10 | 完整 WASI caps + 框架原生 caps + task 创建授权（声明→申请→白名单注入→拒绝未授权）+ 能力校验完整化（未来版本，非 MVP） | v0.2, v0.4 |

## OpenTelemetry 定位

OTel **不是第 4 个功能层**，而是横切可观测性轴：埋点（`tracing` span）织入所有层；OTel SDK + exporter 装配隔离在单一 `shiroha-otel` crate（锁住 0.32 lockstep churn），由 L3 控制器 wiring。分两段：v0.3 起基础 `tracing` 日志，v0.6 完整 OTel export + 跨 worker 传播。

## R7 持久化与崩溃恢复（per-task 可选，v0.3 起）

编排进程重启/崩溃会丢内存中所有 task 实例状态。持久化作为**per-task 可选能力**（控制器创建 task 时声明策略）：

**三种模式**：
- **Realtime（实时持久化）**：每个事件/转换决策/完成事件**同步追加落盘**后再推进。安全/可恢复优先。可配 fsync 强度（`Always` / `EveryN` / `Interval`）。
- **Deferred（延迟持久化）**：事件先入内存，**按时间窗口（`window_ms`）或事件数量（`window_events`）批量落盘**，或仅周期 snapshot（`snapshot_only`）。性能敏感；崩溃丢窗口内进度。
- **None（默认）**：纯内存，最快，不可恢复。

**机制**：
- **Event sourcing**（主路径）：事件流（submitted events + transition decisions + completion events）追加落盘；崩溃后重放重建内存 active state configuration。
- **Snapshot**：周期性把 active state configuration（当前状态集 + in-flight 动作表 + 事件队列）落盘，加速重放。
- `Realtime` = log 末尾重放；`Deferred` = 最近 snapshot + 之后部分 log 重放（`snapshot_only` 则从最近 snapshot，丢窗口）。
- 引擎事件循环在「transition decision 后 / completion event 后」埋持久化 hook；`Realtime` 同步写、`Deferred` 入 batch queue 后台刷盘。
- in-flight 分布式动作崩溃恢复：worker 侧结果按 `dispatch_id` 重新关联重放中的实例（v0.5 起预留 `dispatch_id`，v0.5 起完整化）。
- 存储后端：抽象 `EventStore` trait（append-only log + snapshot read/write）+ `PersistencePolicy` 参数；默认本地文件实现，进阶可换分布式存储。

**v0.3 落地** Realtime + Deferred + None + 恢复；v0.9 增强为多副本 HA（进阶）。

## References

- `glossary.md` — 统一术语表（开发与用户文档用词权威，Phase 3.3 提升到 spec）。
- `research/index.md` — 技术选型结论表 + 6 条关键约束。
- `research/01-wasm-runtime.md` … `06-workspace-layout.md` — 分项评估。
- `design.md` — 技术设计（边界/契约/数据流/IR/Crate 布局）。
- `implement.md` — 执行计划（child task 拆分 + 顺序 + 集成评审）。
