# Shiroha 设计（design.md）

> 父任务 `.trellis/tasks/06-25-shiroha-arch` 的技术设计。产品决策见 `prd.md`（D1–D9），技术选型证据见 `research/01–06`。本文件给出边界、契约、数据流、IR、Crate 布局与风险。具体实现由 child task 承担。

## 1. 架构总览与分层边界

```
┌─────────────────────────── 编排进程 (orchestrator) ───────────────────────────┐
│  shiroha (facade) ──> shiroha-controller (L3)                                  │
│      ├── task CRUD/pause/resume/query, multi-instance hosting                  │
│      ├── auth (token/api-key) + action-capability validation                   │
│      ├── embeds: shiroha-core (L1 engine) + shiroha-scheduler (L2)             │
│      └── shiroha-otel (tracing/OTel subscriber)                                │
│                                                                                │
│  shiroha-core (L1) ──> shiroha-ir (canonical SmIr, serde-only leaf)            │
│  adapters: shiroha-adapter-text | shiroha-adapter-wasm ──> shiroha-ir          │
│  shiroha-scheduler (L2) ──> shiroha-transport (trait)                          │
│  shiroha-transport-grpc (default tonic impl)                                   │
└──────────────▲───────────────────────────────────▲────────────────────────────┘
               │ ActionDispatch (bidi stream)      │ trace context (W3C)
       ┌───────┴───────┐  ┌────────────────┴───────┐ ┌──────────────┐
       │  worker (xN)  │  │  worker (xN)           │ │ OTLP collector│
       │  stateless    │  │  stateless action exec │ └──────────────┘
       │  action exec  │  │  (wasm/shell/http)     │
       └───────────────┘  └────────────────────────┘
```

**层边界（= crate 边界，见 §6）**：
- **L1 = `shiroha-ir` + `shiroha-core` + `shiroha-adapter*`**：IR 是契约，core 是纯引擎（不依赖 wasmtime/tokio/tonic/otel），adapter 把定义解释成 IR。
- **L2 = `shiroha-scheduler` + `shiroha-transport`(+grpc) + `shiroha-worker`**：调度器依赖 `Transport` trait（不依赖 tonic）；worker 是无状态动作执行器。
- **L3 = `shiroha-controller` + `shiroha-otel` + `shiroha` facade**：控制器内嵌 L1+L2，对外暴露统一 API；OTel 隔离于单 crate。

**核心不变量**：
1. `shiroha-ir` = serde-only 叶子，所有层依赖它，它不依赖任何上层。
2. `shiroha-core` 仅依赖 `shiroha-ir`（纯逻辑，无 runtime/wasmtime/network，可独立单测）。
3. wasmtime ≤ 2 crate（`adapter-wasm`、`worker`）；tonic/prost ≤ 2 crate（`transport-grpc`、`worker`）；OTel = 1 crate（`otel`）。
4. 每个「可插拔边界」= trait crate + ≥1 默认实现 crate（adapter、transport）。

## 2. 统一 IR 契约（AC2）

`SmIr` 是引擎唯一消费的类型，serde-derived，位于 `shiroha-ir`。文本 adapter 与 WASM CM adapter 在引擎边界前收敛于 `SmIr`：

```
text ──serde_json/saphyr/toml──> SmIr ──┐            (v0.7 回归，MVP 延后)
                                        ├──> shiroha-core (engine)
wasm ──bindgen!──> MachineDef ──From──> SmIr ──┘     (MVP 首要路径)
```

> **MVP 聚焦 WASM**：文本 adapter（JSON/YAML/TOML）延后到 v0.7。`SmIr` 仍保持 serde-derived，使后期文本 adapter 零成本回归。`shiroha-adapter-text` crate 在 v0.7 才创建。

```rust
// shiroha-ir
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SmIr {
    pub name: String,
    pub root: StateRef,
    pub states: Vec<StateNode>,       // 嵌套 + 并行 region
    pub transitions: Vec<Transition>, // guard / event / source / target / action-refs
    pub actions: Vec<ActionDecl>,     // 命名动作 -> ActionRef
    pub history: Vec<HistoryDecl>,    // 浅历史
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionRef {
    WasmFunc    { export: String },
    Shell       { cmd: String },
    Http        { spec: HttpSpec },
    Plugin      { capability: String, method: String },
    Distributed {
        inner: Box<ActionRef>,
        fanout: Option<u32>,           // 不声明=单点分发(1 worker); N=N 片
        target: Option<TargetSpec>,    // 默认 Any(调度器自主负载均衡)
        aggregate: AggregateRef,       // 内置 Rust 原生 或 wasm 自定义
    },
}

pub enum TargetSpec { Any, Pool(String), Label(String, String), Explicit(Vec<String>) }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AggregateRef {
    Builtin { strategy: BuiltinAggregate },   // Rust 原生,零开销
    Wasm    { export: String },                // 自定义 wasm 聚合器(有状态,CM resource 句柄)
    Plugin  { capability: String, method: String },
}
pub enum BuiltinAggregate { All, Any, Quorum(u32), FirstSuccess }
```

- `StateNode` 编码嵌套 + 正交区域（children: `Vec<Region>`，region 内含子状态）。
- `SmIr` 不是 CM 类型：serde 派生不自动满足 `ComponentType`/`Lift`/`Lower`。WIT world 按 `SmIr` 形状设计，`bindgen!` 生成 `MachineDef`，再以机械的 `From<MachineDef> for SmIr` 收敛（仅 ABI 类型映射，无语义翻译）。这让 `shiroha-adapter-text` 不依赖 wasmtime。
- 文本格式注意：TOML 不擅长深嵌套同质 map，`SmIr` 用 `[states.<id>]` 表形式保持 TOML 友好；TOML 最适合小型机定义。
- **聚合策略可扩展**：内置 4 种用 Rust 原生实现（`{builtin, ...}`，零开销）；自定义走 wasm 插件（`{wasm, export}` 或 `{plugin, ...}`），有状态聚合器经 CM `resource` 句柄（`create() -> handle` / `on-result(handle, one) -> decision` / `destroy(handle)`），受沙箱约束，复用框架插件通道。
- **分发目标控制**：`fanout` 控数量，`target` 控节点约束（`Any`/`Pool`/`Label`/`Explicit`），与 `required_capabilities` 两层过滤。

## 3. WASM Component Model Adapter 契约（AC3）

WIT world（`shiroha-adapter-wasm/shiroha.wit`）：

```wit
world shiroha-machine {
  import host: interface {
    // 插件能力通道：host 按白名单注入。仅声明此机所需 capability。
    http-get: func(url: string) -> result<list<u8>, string>;
    fs-read:  func(path: string) -> result<list<u8>, string>;
    // ... 按 capability 扩展
  }
  export define: func() -> machine-def;          // typed record -> SmIr
  export action-<name>: func(input: list<u8>) -> result<list<u8>, string>;
}
```

**关键约束（research/01）**：`bindgen!` 是编译期从 WIT 生成，而每台机的 action 名是数据（运行期才知）。
→ **策略**：`bindgen!` 只生成 `define()` + host 能力接口；**每动作按名动态解析** `instance.get_func(name).typed::<Vec<u8>, Result<Vec<u8>, String>>()`，对固定 canonical action ABI（`list<u8> -> result<list<u8>, string>`）。不要为每个 action 名 `bindgen!`。

- `define()` 返回 `MachineDef`（typed record）→ `From<MachineDef> for SmIr` → 引擎。
- host 用 `component::Linker` 按本机声明的 capability 白名单注入 host func（未声明的槽位不注册 → 实例化失败 = 自然能力协商）。
- 沙箱：`Config::consume_fuel(true)` + `epoch_interruption(true)` + `StoreLimits`（内存/table/instance 上限）+ `tokio::time::timeout` 包驱动 future；epoch 由独立 tokio interval task 驱动 `Engine::increment_epoch()`。
- `Store` 是 `!Sync` → **每个 state-machine 实例一个 `Store<T>`**，`Engine` 跨实例共享。

## 4. 动作执行模型（D3）

- **结构转换步（同步 RTC）**：选转换、算 LCA、确定 exit/run/enter 顺序——原子完成，不做中途突变。
- **动作异步**：转换结构确定后，触发动作（entry/run/exit action）。动作是 `async`，引擎不阻塞等待；动作完成时抛 `done.<action>`（或失败 `error.<action>`）事件入实例事件队列，驱动后续转换。
- **引擎实现**：`shiroha-core` 用纯逻辑 + `BoxFuture` 表示动作（核心不依赖 tokio）；动作 future 由上层（controller/编排进程）的 tokio runtime 驱动。核心暴露「提交事件 / poll 推进 / 取完成事件」接口，runtime 负责调度。
- **入口动作 invoke 语义**：进入状态后触发其 entry action，完成事件可驱动下一转换（xstate-invoke 风格）。

> 设计要点：`shiroha-core` 不直接 `tokio::spawn`；它产出动作 future 与完成事件，由宿主 runtime 执行并回填结果。这让 core 保持「无 runtime 依赖」可单测。

## 5. 分布调度器（L2，AC5）

- **分发单元** = `ActionRef::Distributed`（`inner` 指真实动作，`fanout` 控数量，`target` 控节点，`aggregate` 指定策略）。
- **分发目标控制（R4.7）**：
  - `fanout: None` = 单点分发（1 worker）；`fanout: Some(N)` = N 片分发。
  - `target: Any`（默认）= 调度器从能执行该动作的 worker 池自主选（负载均衡）；`Pool(name)` = 限定 worker 池；`Label(k,v)` = 按标签（地域/亲和性）过滤；`Explicit([ep...])` = 精确指定端点。
  - 两层过滤：worker 必须满足 `required_capabilities`（能力）**且**满足 `target`（用户偏好）才被选。
- **聚合策略（R4.4）**：
  - **内置 4 种 Rust 原生实现**（`AggregateRef::Builtin`，零开销）：`All` / `Any` / `Quorum(n)` / `FirstSuccess`。编排侧按 `task_id`+`action_ref` 关联结果，按策略聚合。
  - **自定义 wasm 聚合器**（`AggregateRef::Wasm`/`Plugin`）：有状态，经 CM `resource` 句柄。WIT 接口：
    ```wit
    interface aggregator {
      resource aggregator { create: func() -> aggregator; }
      on-result: func(self: borrow<aggregator>, partial: result<list<u8>, string>) -> result<decision, string>;
      drop: func(self: own<aggregator>);  // 析构
    }
    variant decision {
      pending, done(list<u8>), failed(string),
    }
    ```
    每个分片结果到达调 `on-result`，返回 `pending`（继续等）/ `done(payload)`（回流 `done.*`）/ `failed(msg)`（回流 `error.*`）。受沙箱约束（fuel/epoch/StoreLimits），复用框架插件通道。
  - proto 与策略无关：worker 只回 `ActionResult{done|error}`，聚合逻辑全在编排侧。
- **Transport trait**（`shiroha-transport`，无 prost/tonic）：
  ```rust
  #[async_trait]
  pub trait Transport: Send + Sync + 'static {
      type DispatchSink: Sink<ActionDispatch, Error = TransportError> + Send + Unpin + 'static;
      type ResultStream: Stream<Item = Result<ActionResult, TransportError>> + Send + Unpin + 'static;
      async fn connect(&self, ep: &Endpoint) -> Result<(Self::DispatchSink, Self::ResultStream), TransportError>;
  }
  ```
  `ActionDispatch`/`ActionResult` 是 transport-domain 纯 Rust struct（不是 prost 类型）；`shiroha-transport-grpc` 在边界做 prost 映射。
- **gRPC proto**（`proto/shiroha/scheduler/v1/dispatch.proto`）：`service Dispatch { rpc Dispatch(stream ActionDispatch) returns (stream ActionResult); }`，每 worker 一条 bidi 长连接；`required_capabilities` 让 worker 在执行前可拒绝；`ShardHint` 承载 fan-out 分片；`target` 透传给调度器选节点（不进 proto wire，是编排侧选节点逻辑）。聚合策略由编排侧执行（proto 与策略无关）。
- **trace 传播**：编排侧 dispatch 时注入 W3C `traceparent` 到 tonic metadata；worker 侧 extract，建立子 span，使分布式动作 trace 跨 orchestrator+worker 拼接（在 `transport-grpc` 实现，不进抽象 trait）。
- **worker**（`shiroha-worker`）：收到 `ActionDispatch`，按 `action_ref` 执行（wasm via wasmtime / shell / http / plugin），返回 `ActionResult{done|error}`；无跨调用状态。

## 6. Crate / 工作区布局（AC1, AC8）

```
shiroha/ (workspace virtual manifest + [workspace.dependencies] 统一版本)
├── crates/
│   ├── shiroha-ir/            serde-only 叶子；SmIr + ActionRef + Aggregate
│   ├── shiroha-core/          纯引擎：层级+并行 statechart、RTC、转换路径缓存、in-flight 动作/完成事件队列  [仅依赖 ir]
│   ├── shiroha-adapter/       Adapter trait { fn load(&self) -> SmIr }  [仅依赖 ir]
│   ├── shiroha-adapter-text/  JSON/YAML/TOML  [adapter + ir + serde backends]
│   ├── shiroha-adapter-wasm/  bindgen! define()/host caps + From<MachineDef> + 动态 TypedFunc 解析 + 白名单 Linker  [adapter + ir + wasmtime]
│   ├── shiroha-plugin-sdk/    能力 trait + 注册表 + semver 协商 + {plugin,cap.method} ref（纯类型，无 wasmtime）  [仅依赖 ir]
│   ├── shiroha-transport/     Transport trait + ActionDispatch/ActionResult 域类型  [ir + futures + async-trait]
│   ├── shiroha-transport-grpc/ tonic 默认实现 + Dispatch bidi proto + trace 注入/提取  [transport + tonic + prost + tokio]
│   ├── shiroha-scheduler/     分布式 action 分发 + fan-out + 聚合 + 关联器  [ir + transport + tokio]
│   ├── shiroha-worker/        无状态动作执行器（库）  [ir + plugin-sdk + wasmtime + transport-grpc]
│   ├── shiroha-otel/          tracing subscriber + OTLP + 传播助手  [opentelemetry*0.32 + tracing-opentelemetry0.33]
│   ├── shiroha-controller/    L3：task CRUD/pause/resume、多实例托管、auth、能力校验、内嵌 core+scheduler  [core+scheduler+adapter*+plugin-sdk+transport-grpc+otel+ir]
│   └── shiroha/               facade：电池齐全默认栈  [controller + 默认 adapter/transport/otel]
├── bin/
│   ├── shiroha-orchestrator/  编排进程二进制  [shiroha]
│   └── shiroha-worker-bin/     worker 二进制  [shiroha-worker]
└── proto/                     dispatch.proto (+ tonic-build in build.rs)
```

依赖方向严格向下（DAG 无环），见 research/06 的依赖图。`[workspace.dependencies]` 统一 pin：`serde`、`tokio`、`wasmtime`、`opentelemetry*`、`tracing`、`anyhow`、`thiserror`。

**Feature flag**：facade `default = ["text-adapters","wasm-adapter","grpc-transport","otel-otlp"]`，可关单项瘦身（如 `--no-default-features --features text-adapters` 的无 wasm 构建）；`transport-grpc` 的 TLS（`tls-ring`/`tls-aws-lc` + roots）默认关，R5.5 按需开。

## 7. 控制器与可观测性（AC6）

> **OTel 定位**：OpenTelemetry **不是第 4 个功能层**，而是横切可观测性轴。埋点（`tracing` span）织入所有层（core/adapter/scheduler/worker/controller）；OTel SDK + exporter 装配隔离在单一 `shiroha-otel` crate（锁住 0.32 lockstep churn），由 L3 控制器作为进程宿主 wiring。分两段上线：v0.3 起基础 `tracing` 日志（无 exporter），v0.6 完整 OTel export + 跨 worker trace 传播。

- **`Controller` API**（`shiroha-controller`）：`create_task(machine_def, input) -> TaskId`、`pause/resume/cancel/query`、`submit_event(task_id, event)`、`list_tasks(filter)`。GUI/CLI 经进程内 facade；Web 经 service boundary（同 API，gRPC 或 HTTP 暴露在 child task 定）。
- **多实例托管**：每 task 一个 `Store<T>`（wasm 时）+ 独立事件队列 + 一个 root `tracing::Span`（属性 `task_id`+`machine_name`）。多线程 runtime 上每实例的 store 用 `LocalSet` 或 `Arc<Mutex<Store>>`（research/02 约束）。
- **OpenTelemetry**（`shiroha-otel`，research/05）：`tracing` 作唯一埋点 API；`shiroha-otel` 装 subscriber（Registry + `tracing_opentelemetry::layer` + `MetricsLayer`(feature) + `opentelemetry_appender_tracing`）+ OTLP/gRPC exporter。**0.32 lockstep**：所有 `opentelemetry*` + `tracing-opentelemetry0.33` 同 0.32.x，一起升。trace 生产可用，metrics 有 churn，logs bridge 实验性 → 发布顺序：trace → metrics → logs。
- **metrics**：counter `shiroha.tasks.created`、up-down `shiroha.tasks.active`、histogram `shiroha.action.duration`/`shiroha.transition.duration`，按 task/machine/action/worker 维度。

## 8. 安全模型（D8, R5.5）

- **认证**：控制器 API token / API-key（metadata 传递）；worker 共享 token（gRPC metadata），TLS 模式下可升 mTLS。
- **动作能力校验**：状态机定义声明所用 capability；host 按策略允许/拒绝（白名单 Linker 注入；未声明槽位不注册 → 实例化失败）；worker 收到 `required_capabilities` 时先校验自身能否执行再执行。
- **传输加密**：`shiroha-transport-grpc` 的 rustls features 按需开启（orchestrator↔worker、controller↔Web）。
- **沙箱**：wasm 动作 + wasm 聚合器受 fuel + epoch + StoreLimits + timeout 约束（§3, §5）。
- **进阶（非 MVP）**：RBAC、多租户隔离、多副本 HA。

## 8a. 持久化与崩溃恢复（per-task 可选，v0.3 起）

编排进程是单点（D6）；崩溃丢内存中所有实例状态。持久化作为**per-task 可选能力**（控制器创建 task 时声明 `PersistencePolicy`，不默认强加落盘开销）：

**三种模式**：
```rust
pub enum PersistencePolicy {
    None,                                                  // 默认:纯内存,最快,不可恢复
    Realtime   { fsync: FsyncPolicy },                     // 实时:每事件同步落盘
    Deferred   { window: WindowSpec, snapshot_only: bool }, // 延迟:批量或仅 snapshot
}
pub enum FsyncPolicy { Always, EveryN(u32), Interval(Duration) }
pub enum WindowSpec   { Time(Duration), Events(u32) }
```

- **Realtime**：安全/可恢复优先。每个事件/转换决策/完成事件**同步追加落盘**后再推进；可配 fsync 强度。崩溃后从 log 末尾重放，零丢失。
- **Deferred**：性能敏感。事件先入内存，按时间窗口或事件数批量落盘，或仅周期 snapshot（`snapshot_only`）。崩溃丢窗口内进度。恢复 = 最近 snapshot + 之后部分 log（snapshot_only 则仅最近 snapshot）。
- **None**：默认，纯内存，不可恢复。

**机制**：
- **Event sourcing（主路径）**：每实例事件流追加落盘——submitted events + transition decisions（selected transition, exit/run/enter action 触发记录）+ completion events（`done.*`/`error.*`）。崩溃后重放重建内存 active state configuration。与「异步动作+完成事件」模型天然契合（事件流已是引擎核心数据结构）。
- **Snapshot**：周期性把 active state configuration（当前状态集 + in-flight 动作表 + 事件队列）落盘，加速重放。
- 引擎事件循环在「transition decision 后 / completion event 后」埋持久化 hook：`Realtime` 同步写 + fsync；`Deferred` 入 batch queue 由后台 tokio task 按窗口刷盘。
- **分布式动作恢复**：in-flight distributed action 崩溃后，worker 侧结果按 `task_id`+`action_ref`+`dispatch_id` 重新关联到重放中的实例；未确认结果丢弃重发。v0.5 起预留 `dispatch_id` 字段并完整化关联。
- **存储后端**：抽象 `EventStore` trait（append-only log + snapshot read/write）+ `PersistencePolicy` 参数；默认本地文件实现（sled/rocksdb 可选），进阶可换分布式存储。
- **v0.3 落地** Realtime + Deferred + None + 恢复；v0.9 增强为多副本 HA（进阶）。

## 9. 技术选型结论（AC7）

| 主题 | 选定 | 理由一行 | 备选 |
|---|---|---|---|
| WASM 运行时+CM | `wasmtime` 46.x | 唯一满足全部 6 需求（typed bindgen! + Linker + fuel/epoch/StoreLimits + async + Apache-2.0） | `wasm_component_layer`（仅作未来非 wasmtime 后端） |
| 异步运行时 | `tokio` | tonic 强制；wasmtime async 完美组合；驱动 epoch + OTLP | 无实际替代 |
| 序列化/IR | `serde`+`serde_json`+`serde-saphyr`+`toml` | 统一 `SmIr`；⚠️ `serde_yaml`/`serde_yml` 已弃用，用 `serde-saphyr` | `noyalib`（YAML） |
| 传输 | `tonic`0.14 + `prost`0.14 | bidi `Dispatch` 流；抽象 trait 可换 libp2p/QUIC | quinn/libp2p（实现同 trait） |
| 可观测性 | `tracing`0.1 + OTel 0.32 family | 标准 Rust OTel 栈；隔离 `shiroha-otel` | `-stdout`/`-prometheus` 替换 exporter |
| 工作区 | 12-crate workspace | 层=crate 边界；core 零上游依赖；重依赖局部化 | ir 折入 core（更粗） |

## 10. 关键风险与缓解

| 风险 | 影响 | 缓解 |
|---|---|---|
| OTel 0.x 每次 minor 破坏 `tracing-opentelemetry` | 编译错误 | 隔离 `shiroha-otel`，pin 0.32 lockstep，集中升级 |
| YAML crate 生态迁移中 | 选到弃用 crate | 用 `serde-saphyr`，YAML 隔离在 `YamlAdapter` 后，pin 前复核版本 |
| `bindgen!` 编译期 vs action 名是数据 | 不能为每个 action 静态生成 | `define()`+host 用 bindgen!；action 按名动态 `typed::<In,Out>` 走固定 ABI |
| `prost` 需 `protoc` 构建期 | CI/devshell 缺 protoc 构建失败 | 文档要求 protoc，或 `protoc-bin-vendored` hermetic |
| wasmtime `Store` `!Sync` vs 多线程 runtime | 实例 store 共享难 | 每实例一个 Store；`LocalSet`/`Arc<Mutex<Store>>`（research/02） |
| `component-model-async` 仍在演进 | lift/lower ABI 变 | pin 精确 minor，跟踪 wasmtime release |
| 编排进程单点 | 故障丢内存中实例状态 | MVP 接受；v0.9 持久化（event-sourcing+snapshot）可选开启；多副本 HA 列进阶 |
| wasm 聚合器有状态句柄生命周期 | 句柄泄漏/崩溃 | 用 CM resource + Drop 语义；v0.4/v0.5 测句柄生命周期 |
| 自定义聚合器死循环/超时 | 聚合不完成 | fuel/epoch + 超时包 on-result；超时→`error.*` |
| tonic bidi 背压 | worker 喂死 | scheduler 尊重 `Sink::poll_flush`/credit，谨慎设计关联循环 |

## 11. 兼容性 / 演进

- IR 是稳定契约：新 adapter（如未来 SCXML/JSON-Schema）只需产 `SmIr`，零 core 改动。
- Transport 是稳定契约：libp2p/QUIC 实现同 trait 即可替换。
- 插件能力可扩展：新增 capability = 加 host func + 白名单项，不改框架核心。
- 深历史 / 持久化恢复 / RBAC / 多副本 HA 为预留扩展点，不在 MVP 实现但 IR/API 预留空间。

## 12. 待 child task 细化的点

- per-event 性能指标基准与目标值（R1.1）。
- 节点发现 / 重试 / 一致性策略具体方案（R4.6）。
- controller service boundary 的 Web 协议（gRPC vs HTTP/REST）选型。
- 持久化/恢复的可选实现形态（event-sourcing? snapshot?）——进阶。
- 深历史是否纳入 MVP 的最终决定。
