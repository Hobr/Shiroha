# Design — 第一层状态机核心与 WASM Component Model adapter

> 父任务：`06-29-shiroha-framework`。本文记录第一层 MVP 的架构边界、契约、数据流与权衡。决策依据见 `prd.md` 的 D1–D5。

## 1. 范围与决策回顾

- D1 形式化 = 层级 HSM（嵌套状态 + entry/exit action + history），task 实例 actor 风格；MVP 无正交/并发区域，IR 预留扩展位。
- D2 action 模型 = 同步副作用 action（entry/exit/transition）+ 每状态至多一个 async do-activity；迁移走 RTC。
- D3 WASM adapter 提取契约 = 声明式细粒度 WIT 接口（运行时分片查询组装 IR）；action/callback 为组件内单独导出，按名引用。
- D4 持久化 = MVP 不做（纯内存），task 状态可序列化。
- D5 capability = MVP 仅在 task 创建边界留 `Authorizer` trait 接缝（默认 no-op）；host import 面即未来 capability 面。

## 2. 架构边界与 crate 布局

```
Cargo.toml                  # workspace (resolver 3)
crates/
  ir/          shiroha-ir        # IR 数据 schema: state/transition/event/action-ref/history. 纯类型, 无运行时依赖.
  engine/      shiroha-engine    # HSM 运行时: 状态树/迁移/RTC/history; trait 接缝(Adapter/ActionInvoker/Plugin/Authorizer); Task actor + mailbox; TaskManager(控制面).
  wasm/        shiroha-wasm      # wasmtime Component Model adapter: 实现 Adapter + ActionInvoker; host import.
  plugin/      shiroha-plugin    # Plugin trait + registry + 内置 http action func(MVP 一种).
  control/     shiroha-control   # gRPC 控制面: proto 类型 + tonic client/server stubs + service impl(ShirohaControl + NodeExecutor). 依赖 engine trait 接缝.
bin/
  shirohad/    shirohad          # 守护进程: cargo feature 三形态(full/controller/node). full=controller+本地node; controller=控制端; node=无状态执行端.
  sctl/        sctl              # 控制 CLI: clap + gRPC client(调 shirohad 控制面).
wit/                          # WIT 定义: definition world + action 契约 + host import. 供 host 生成绑定 + 示例组件生成 guest 绑定.
proto/                        # protobuf: shiroha.proto 控制面 service 定义. 供 tonic-prost-build 生成.
examples/
  sm-example/                   # wasm32-wasip2 示例组件, 集成测试用.
```

依赖方向（单向、无环）：

```
ir  ←  engine  ←  wasm ( + wasmtime )
               ←  plugin
               ←  control ( + tonic )
shirohad ← engine + wasm + plugin + control
sctl     ← control (仅 client stubs + proto 类型)
```

- `shiroha-engine` 定义 trait 接缝，**不依赖** wasmtime；`shiroha-wasm` 实现 trait，把 wasmtime 限制在该 crate 内。
- `shiroha-ir` 无依赖，可被 `shiroha-wasm` 与未来文件 adapter 共享。
- WASM 宿主绑定与 guest 绑定共用 `wit/` 定义。
- `shiroha-control` 依赖 engine trait 接缝（消费 TaskManager + Adapter + Authorizer + PluginRegistry），不直接依赖 wasmtime；`sctl` 仅用其 client stubs（不拉 engine 运行时）。

## 3. 内部 IR schema（`shiroha-ir`）

核心类型（伪 Rust，最终形态在实现时确定）：

```rust
pub struct StateMachineDef {
    pub name: String,
    pub initial: StateId,
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
    pub events: Vec<EventDef>,
}

pub struct State {
    pub id: StateId,
    pub parent: Option<StateId>,      // 嵌套
    pub entry: Option<ActionRef>,
    pub exit: Option<ActionRef>,
    pub do_activity: Option<ActionRef>,   // 至多一个 async do-activity
    pub history: HistoryConfig,            // None | Shallow | Deep
    // 预留: ortho: Option<OrthogonalRegion> (MVP 不用)
}

pub struct Transition {
    pub from: StateId,
    pub to: StateId,
    pub event: EventId,
    pub guard: Option<GuardRef>,
    pub action: Option<ActionRef>,         // 同步 transition action
}

pub struct ActionRef {
    pub name: String,
    pub kind: ActionKind,                  // Wasm | Plugin
}

pub enum GuardRef { Always, Wasm(String), Plugin(String) }
pub enum HistoryConfig { None, Shallow, Deep }
```

- IR 是 adapter 产出的统一中间表示；WASM adapter 与未来文件 adapter 都产出 `StateMachineDef`。
- `do_activity` 与 transition/entry/exit 的 action 都用 `ActionRef`，区分同步/异步由**位置语义**决定（do_activity 位置 = async；其余 = sync），而非类型字段，避免误用。

## 4. 事件模型 / RTC / Task 实例（`shiroha-engine`）

**事件分类（全部汇入 task 的单一 mailbox 队列）：**
- 外部事件：经 `TaskHandle::send(event)` 投递。
- 定时器事件：运行时用 `tokio::time` 调度，到点作为事件入队。
- 内部完成事件：do-activity 完成时产生完成信号入队。

**RTC 语义：**
- 一次只从 mailbox 取一个事件，完整处理其触发的迁移链（含嵌套 entry/exit 级联）后才取下一个事件。
- 同步 action 在 RTC 内执行（阻塞当前事件处理，必须快）。

**do-activity 与 RTC 的关系：**
- 进入状态时启动该状态的 do-activity（作为独立 tokio task 运行，不阻塞 RTC）。
- 事件导致退出状态时，取消该 do-activity（tokio task 取消）。
- do-activity 完成产生内部完成事件入队，可触发后续迁移。

**并发模型：**
- 单个 task **串行**处理自身事件（无 task 内并行），保证确定性与可推理。
- 多个 task 之间并发（各自 tokio task）。

**可寻址：** 每个 task 有 `TaskId` + `TaskHandle`（clone-able sender）；`TaskHandle::send(Event)` 是唯一外部入口。

## 5. Trait 接缝（`shiroha-engine`，实现侧反向依赖）

```rust
// 产出 IR（adapter 实现此 trait）
#[async_trait]
pub trait Adapter: Send + Sync {
    async fn load(&self) -> Result<StateMachineDef>;
}

// 执行 action（wasm 侧与 plugin 侧各实现）
#[async_trait]
pub trait ActionInvoker: Send + Sync {
    async fn invoke_sync(&self, name: &str, ctx: ActionContext) -> Result<ActionResult>;
    async fn invoke_do(&self, name: &str, ctx: ActionContext) -> Result<ActionResult>;  // 可取消
}

// plugin = 通用框架扩展点系统(见 §8)
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn register(&self, registry: &mut PluginRegistry);
}

// capability 留口（MVP no-op default impl）
#[async_trait]
pub trait Authorizer: Send + Sync {
    async fn authorize(&self, req: AuthorizeReq) -> Result<(), AuthzError>;  // 默认 Ok
}
```

- engine 依赖 trait，不依赖具体实现；`shiroha-wasm` 提供 `WasmAdapter` + `WasmActionInvoker`。
- 这套 trait 也是第二层（分布调度）的接入点：调度器实现 `ActionInvoker::invoke_do` 把 do-activity 分发到远端节点。
- `Plugin` 不直接定义能力，而是把自身能力注册进 `PluginRegistry`（按能力面 typed 存取）；具体能力面见 §8。

## 6. WASM adapter 契约（WIT 草图）

组件实现一个声明式细粒度 `definition` 接口，运行时分片查询组装 IR；action 为组件内单独导出函数。

`wit/state-machine.wit`（草图）：

```wit
package shiroha:sm;

interface types {
  variant action-kind { %wasm(string), %plugin(string) }
  variant guard-kind   { always, %wasm(string) }
  enum   history-kind  { none, shallow, deep }
  record action-ref    { name: string, kind: action-kind }
  record state {
    name: string, parent: option<string>,
    entry: option<action-ref>, exit: option<action-ref>,
    do-activity: option<action-ref>, history: history-kind,
  }
  record transition {
    from: string, to: string, event: string,
    guard: option<guard-kind>, action: option<action-ref>,
  }
  record event-def { name: string }
}

interface definition {
  initial: func() -> string;
  states: func() -> list<types::state>;
  transitions: func() -> list<types::transition>;
  events: func() -> list<types::event-def>;
}

// 统一 action 签名(同步 action): ctx 进, result 出
interface actions {
  invoke: func(ctx: action-context) -> result<action-result, string>;
  // do-activity 用 component-model async future(见 §9 风险)
  invoke-do: func(ctx: action-context) -> future<result<action-result, string>>;
}

record action-context {
  task-id: string,
  event: option<string>,
  payload: option<list<u8>>,   // 事件负载(MVP 用 bytes; 未来结构化)
}
variant action-result {
  ok,
  ok-value(list<u8>),
  error(string),
  signal(string),             // 可回灌迁移的内部信号
}

world state-machine {
  export definition;          // 声明式定义
  export actions;              // action 实现
  import host: host-interface; // host 能力(= 未来 capability 面)
}

interface host-interface {
  log: func(level: u8, msg: string);
  // MVP 仅 log; 未来 http/kv... 作为 capability 按需导入
}
```

**运行时流程（adapter）：**
1. 加载组件 → 实例化（链接 host-interface）。
2. 调 `definition.initial/states/transitions/events` → 组装 `StateMachineDef`（IR）。
3. 处理事件时，按 `ActionRef.name` 调 `actions.invoke`（同步）或 `actions.invoke-do`（async future）。

## 7. action 调用与 host import

- **统一签名**：所有 WASM action 走 `actions.invoke(ctx) -> result<action-result, string>`；do-activity 走 `invoke-do` 返回 component-model future。action 名通过 `ActionRef.name` 映射——但 WIT 单一 `invoke` 无法按名分发，故约定：**action 按名映射到组件的不同导出**（每个 action 一个导出函数，统一签名），或单一 `invoke` + `ctx.action_name` 分发。MVP 取后者（单 `invoke` + 名称分发），更简单、单点注册。
- **host import**：MVP 仅 `host.log`。host import 面 = 未来 capability 面（D5 的延伸）：未来 http/kv/网络等以 capability 形式按 task 授权注入。
- **结果回灌**：`action-result::signal(name)` 可产生一个内部事件入 mailbox，从而影响后续迁移（满足「action 结果回灌迁移」）。

## 8. Plugin 扩展点系统（`shiroha-plugin` + `shiroha-engine`）

> v0.3.0 重点：架构就位，接口留口。优先让 WASM action 真正跑起来（v0.2.5），plugin 具体实现（如 HTTP func）推迟到 v0.3.5+。

Plugin 是通用框架扩展点系统：一个插件实现 `Plugin` trait，在 `register()` 中把自身能力按**能力面（extension point）**注册进 `PluginRegistry`。注册表按能力面 typed 存取，运行时按需查询。

### 8.1 能力面（ExtensionPoint）

| 能力面 | 作用 | 所属层 | v0.3.0 状态 |
|---|---|---|---|
| `ActionFunc` | 提供 action 实现源（http / bash / custom…），作为 `ActionInvoker` 的一种实现源 | 第一层 | **仅 trait 定义** |
| `Middleware` | 横切关注点（日志 / 监控 / 追踪 / 限流），包绕 action 调用或事件处理 | 第一/三层 | **仅 trait 定义** |
| `AggregationStrategy` | 定义 do-activity 结果聚合策略（map/reduce/first-wins…） | 第二层 | **仅 trait 定义** |
| `Transport` | 分布式协议（rpc / p2p / 消息服务…），作为第二层分发节点间通信与任务派发载体 | 第二层 | **仅 trait 定义** |
| `Adapter` | 扩展状态机定义来源（用户可自定义 IR 适配器，如从 DB / 配置中心 / 远程 API 加载定义） | 第一层 | **仅 trait 定义** |

> 能力面集合开放：未来可新增（如 `Serializer`/`Storage` 持久化后端等），不破坏已有插件。

### 8.2 两层语义设计（ActionFunc 示例）

**Plugin 类型层**：`ActionRef { kind: Plugin("http"), name: "fetch_user" }`
- `"http"` → plugin 类型名，映射到具体 `ActionFunc` 实现
- `"fetch_user"` → action 实例名，传递给 `ActionFunc::invoke()`

**实现侧解释**：
- Registry 存储 `HashMap<String, Arc<dyn ActionFunc>>`，key = plugin 类型名（"http", "bash"）
- `ActionFunc::invoke(ctx)` 内部根据 `ctx` 或其他途径（如 payload）解释 action 实例名
- 配置可从 `ctx.payload` 动态传递（如 HTTP URL/method），无需预注册每个 action 实例

**示例**：
```rust
// IR 定义
ActionRef { kind: Plugin("http"), name: "fetch_user" }

// 运行时
let func = registry.action_func("http")?;  // 查找 "http" 类型的 ActionFunc
let result = func.invoke(ctx).await;        // ctx.payload 包含 HTTP 配置 (URL/method/headers)
```

### 8.3 PluginRegistry 设计

#### 基础结构

```rust
pub trait Plugin: Send + Sync {
    fn name(&self) -> &str;
    fn register(&self, registry: &mut PluginRegistry);
}

pub struct PluginRegistry {
    // 按能力面 typed 存取; 每个能力面一个独立 trait
    action_funcs: HashMap<String, Arc<dyn ActionFunc>>,
    middlewares: Vec<Arc<dyn Middleware>>,
    aggregation_strategies: HashMap<String, Arc<dyn AggregationStrategy>>,
    transports: HashMap<String, Arc<dyn Transport>>,
    adapters: HashMap<String, Arc<dyn Adapter>>,
}
```

#### 线程安全与共享策略

**v0.3.0 MVP 选择**：`Arc<PluginRegistry>`（不可变，无锁）

```rust
// 初始化阶段（可变）
let mut registry = PluginRegistry::new();
registry.register_action_func("http", Arc::new(HttpActionFunc::new()));
registry.register_action_func("bash", Arc::new(BashActionFunc::new()));

// Freeze 并共享（不可变）
let registry = Arc::new(registry);

// 运行时（只读访问，无锁）
let func = registry.action_func("http")?;
```

**理由**：
- MVP 不需要热更新（v0.3.0 范围外）
- 简单高效，运行时零锁开销
- 初始化流程清晰：构建 → freeze → 共享
- 未来如需热更新，可演进为内部 `RwLock` 包裹各容器

#### Registry API

```rust
impl PluginRegistry {
    pub fn new() -> Self;
    
    // 查询 API（运行时）
    pub fn action_func(&self, name: &str) -> Option<Arc<dyn ActionFunc>>;
    pub fn middlewares(&self) -> &[Arc<dyn Middleware>];
    pub fn aggregation_strategy(&self, name: &str) -> Option<Arc<dyn AggregationStrategy>>;
    pub fn transport(&self, name: &str) -> Option<Arc<dyn Transport>>;
    pub fn adapter(&self, name: &str) -> Option<Arc<dyn Adapter>>;
    
    // 注册 API（初始化阶段）
    pub fn register_action_func(&mut self, name: impl Into<String>, f: Arc<dyn ActionFunc>);
    pub fn register_middleware(&mut self, m: Arc<dyn Middleware>);
    pub fn register_aggregation_strategy(&mut self, name: impl Into<String>, s: Arc<dyn AggregationStrategy>);
    pub fn register_transport(&mut self, name: impl Into<String>, t: Arc<dyn Transport>);
    pub fn register_adapter(&mut self, name: impl Into<String>, a: Arc<dyn Adapter>);
}
```

### 8.4 能力面 trait 定义

#### ActionFunc（v0.3.0 仅定义，无内置实现）

```rust
#[async_trait]
pub trait ActionFunc: Send + Sync {
    /// Invoke an action with the given context.
    /// 
    /// Implementation receives:
    /// - `ctx.payload`: Configuration data (e.g., HTTP URL/method for HTTP func)
    /// - Action instance name can be embedded in payload or derived from ctx
    async fn invoke(&self, ctx: ActionContext) -> anyhow::Result<ActionResult>;
}
```

**配置传递策略**：
- 从 `ctx.payload` 解析配置（如 HTTP 的 URL/method/headers）
- 灵活：每次调用可有不同配置
- 动态：配置可在状态机定义时嵌入

**未来扩展（v0.3.5+）**：HTTP ActionFunc 实现
```rust
pub struct HttpActionFunc { /* reqwest client */ }

#[derive(Deserialize)]
struct HttpConfig {
    url: String,
    method: HttpMethod,  // enum: GET/POST/PUT/DELETE
    headers: Option<HashMap<String, String>>,
    body: Option<Vec<u8>>,
    timeout_secs: Option<u64>,
}

impl ActionFunc for HttpActionFunc {
    async fn invoke(&self, ctx: ActionContext) -> Result<ActionResult> {
        let config: HttpConfig = serde_json::from_slice(ctx.payload.as_ref().unwrap())?;
        // 执行 HTTP 请求
        // 所有错误（网络/4xx/5xx）统一映射到 ActionResult::Error
    }
}
```

#### Middleware（v0.3.0 仅定义，无链式调用实现）

```rust
#[async_trait]
pub trait Middleware: Send + Sync {
    /// Wrap an action invocation with middleware logic.
    /// 
    /// v0.3.0: Trait 定义占位，链式调用逻辑推迟到第三层（可观测性）。
    async fn wrap_action(
        &self,
        ctx: &ActionContext,
        next: /* 占位类型，未来定义为 BoxFuture chain */,
    ) -> anyhow::Result<ActionResult>;
}
```

**注**：v0.3.0 不实现 `MiddlewareChain` 洋葱调用逻辑，仅定义 trait 预留接口。

#### 其他能力面（v0.3.0 仅定义）

```rust
#[async_trait]
pub trait AggregationStrategy: Send + Sync {
    /// Aggregate multiple action results (第二层：分布式聚合)。
    async fn aggregate(&self, results: Vec<ActionResult>) -> anyhow::Result<ActionResult>;
}

#[async_trait]
pub trait Transport: Send + Sync {
    /// Dispatch a do-activity to remote node (第二层：分布式传输)。
    async fn dispatch(&self, task: &str, activity: &str, ctx: ActionContext) -> anyhow::Result<ActionResult>;
}

// Adapter 复用 §5 的 Adapter trait（engine crate 已定义）
// 插件可注册自定义 adapter（如文件 adapter: JSON/TOML）
```

### 8.5 ActionInvoker 集成

#### 接口修改（传递 ActionRef）

**当前签名（v0.2.0）**：
```rust
async fn invoke_sync(&self, name: &str, ctx: ActionContext) -> Result<ActionResult>;
```

**新签名（v0.3.0）**：
```rust
async fn invoke_sync(&self, action_ref: &ActionRef, ctx: ActionContext) -> Result<ActionResult>;
async fn invoke_do(&self, action_ref: &ActionRef, ctx: ActionContext) -> Result<ActionResult>;
```

**理由**：
- `ActionRef` 包含完整信息（`kind: Wasm | Plugin(name)`, `name`）
- CompositeActionInvoker 根据 `kind` 路由到正确实现
- 语义清晰，扩展友好

#### CompositeActionInvoker（Wasm + Plugin 路由）

```rust
pub struct CompositeActionInvoker {
    wasm_invoker: Arc<WasmActionInvoker>,
    plugin_registry: Arc<PluginRegistry>,
}

impl CompositeActionInvoker {
    pub fn new(wasm_invoker: Arc<WasmActionInvoker>, plugin_registry: Arc<PluginRegistry>) -> Self {
        Self { wasm_invoker, plugin_registry }
    }
}

#[async_trait]
impl ActionInvoker for CompositeActionInvoker {
    async fn invoke_sync(&self, action_ref: &ActionRef, ctx: ActionContext) -> anyhow::Result<ActionResult> {
        match &action_ref.kind {
            ActionKind::Wasm => {
                // 路由到 WASM invoker
                self.wasm_invoker.invoke_sync(action_ref, ctx).await
            }
            ActionKind::Plugin(plugin_type) => {
                // 查找 plugin registry
                let func = self.plugin_registry.action_func(plugin_type)
                    .ok_or_else(|| anyhow!("Plugin not found: {}", plugin_type))?;
                func.invoke(ctx).await
            }
        }
    }
    
    async fn invoke_do(&self, action_ref: &ActionRef, ctx: ActionContext) -> anyhow::Result<ActionResult> {
        // 同上逻辑
        match &action_ref.kind {
            ActionKind::Wasm => self.wasm_invoker.invoke_do(action_ref, ctx).await,
            ActionKind::Plugin(plugin_type) => {
                let func = self.plugin_registry.action_func(plugin_type)?;
                func.invoke(ctx).await  // ActionFunc 不区分 sync/do，内部自行处理
            }
        }
    }
}
```

### 8.6 WASM 一等公民的延伸

插件能力面未来可由 WASM 组件实现：
- **ActionFunc**：WASM 组件导出 action func 并注册（区别于状态机定义中的 WASM action）
- **AggregationStrategy**：WASM 定义分布式聚合策略
- **Transport / Adapter**：只要 trait 能由 WASM 实现并注册即可

> 层次区分：
> - **状态机定义中的 WASM action**：`ActionRef { kind: Wasm, name: "on_entry" }` → 状态机结构的一部分
> - **Plugin 形式的 WASM action**：`ActionRef { kind: Plugin("wasm_custom"), name: "..." }` → 框架能力扩展
>
> 两者可共用 WASM 运行时，但语义不同。

### 8.7 v0.3.0 MVP 落地范围

**实现内容**：
- ✅ `PluginRegistry` 结构定义 + API（Arc 共享，不可变）
- ✅ `Plugin` trait 定义
- ✅ 五个能力面 trait 定义（ActionFunc / Middleware / AggregationStrategy / Transport / Adapter）
- ✅ `ActionInvoker` 接口修改（传递 `ActionRef`）
- ✅ `CompositeActionInvoker` 路由实现（Wasm + Plugin）
- ✅ 单元测试：空 registry 查找、路由逻辑

**不包含（推迟到后续版本）**：
- ❌ HTTP ActionFunc 实现（→ v0.3.5 或 v0.4.0）
- ❌ Middleware 链式调用逻辑（→ 第三层：可观测性）
- ❌ 其他能力面的内置实现

### 8.8 版本演进路径

**v0.2.5（新增，优先）**：完整 WASM action 执行
- 实现 `WasmActionInvoker`（替换当前占位）
- 简单示例 WASM action（如 log 或 counter）
- 端到端验证：state machine → WASM action → 结果验证

**v0.3.0**：Plugin 架构就位
- 本节（§8）设计内容全部落地
- 架构可用，但无具体插件实现

**v0.3.5 / v0.4.0**：首个具体插件
- HTTP ActionFunc 实现（基于 `reqwest`）
- 配置结构：`HttpConfig { url, method, headers, body, timeout_secs }`
- 错误处理：所有 HTTP 错误统一映射到 `ActionResult::Error`

**未来**：
- Middleware 链式调用（可观测性层）
- 更多内置 plugin（bash func, file adapter, 等）
- WASM 形式的插件支持

## 9. 权衡与风险

- **WASM do-activity 的 async**：依赖 wasmtime component-model async（`wasmtime` 的 `async`/`component-model-async`）。wasmtime 46 已支持 component model async，但成熟度需在实现期验证。**回退方案**：若 async 不稳，MVP 的 do-activity 限定为「同步计算 + 运行时包裹为可取消 tokio task」，长跑/分发交由 host plugin do-activity 承担，WASM do 仅做短计算。implement 阶段先验证此风险点。
- **细粒度 WIT 的多次调用**：`definition` 分片查询为多次调用，但仅定义加载时一次，之后 IR 缓存；可接受。
- **统一 action 签名 vs 多样签名**：统一签名牺牲灵活性换简单与可插拔；结构化 payload 留待未来（MVP 用 bytes + 名称分发）。
- **history 的 deep 实现**：deep history 需记录完整嵌套活跃路径，比 shallow 复杂；MVP 两者都做但 deep 限定单条活跃路径（无正交，路径唯一）。
- **性能**：release profile 已 LTO/strip/opt3；HSM 转移查找用 `HashMap`/`smallvec` 热路径优化；事件队列用 `mpsc`。单 task 串行避免锁。

## 10. 兼容性 / 演进

- IR 预留正交区域字段（`State::ortho`），未来加正交不破坏 schema。
- `Authorizer`/`Plugin`/`Adapter` trait 用对象安全设计，未来扩展不破坏实现侧。
- **Plugin 能力面集合开放**：新增能力面只需在 `PluginRegistry` 加一个 typed 容器 + 对应 trait，不破坏已有插件（见 §8）。
- WIT 接口版本化（`shiroha:sm` 包版本），组件与 host 绑定同源。
- 文件 adapter（JSON/TOML）只需实现 `Adapter` trait 并通过 `Plugin` 注册进 registry，或直接作为内置 adapter，无需改 engine。

## 11. MVP 验证形态（与 acceptance 对齐）

- 一个 `examples/sm-example` wasm32-wasip2 组件实现 `definition` + `actions` + `host` 导入。
- `shirohad`（或测试用 in-process）加载组件 → 产 IR → 创建 task → 注入事件 → 验证：状态迁移正确、entry/exit 执行、guard 阻断、do-activity 启动/取消、history 恢复。
- `just check` / `just test`（cargo-nextest）通过。

## 12. 控制面 / 服务端边界契约（sctl ↔ shirohad）

> 边界契约现在定义（跨层决策），实现在 v0.4.0。原则：控制面是 engine 的**消费者**，通过 trait 接缝调用，绝不绕过直达内部。

### 12.1 shirohad 三形态与进程架构

shirohad 通过 **cargo feature** 编译为三种形态——同一二进制、同一代码库、按需裁剪：

| feature | 形态 | 包含 | 用途 |
|---|---|---|---|
| `controller` | 控制端 | gRPC server + 控制面 + TaskManager + Adapter + Authorizer + PluginRegistry | 较重，单点控制；do-activity 可本地执行或分发到 node |
| `node` | 节点 | NodeExecutor gRPC service + 本地 ActionInvoker（WASM）+ PluginRegistry(子集) | 无状态执行端；注册到 controller，接收分发的 do-activity 执行并返回结果 |
| `full` | 全量 | controller + 本地 node（node 实例注册到本进程 controller） | 单机开箱即用：控制端 + 内置执行节点 |

**三形态进程架构：**

```
形态 controller:                          形态 node:
┌──────── shirohad ────────┐             ┌──────── shirohad ────────┐
│ gRPC server (控制面)     │  ←分发 do─  │ NodeExecutor gRPC service│
│  ├ ShirohaControl impl   │  ─activity→ │  ├ 接收 do-activity 请求 │
│  ├ TaskManager           │             │  ├ 本地 ActionInvoker   │
│  ├ Adapter/Authorizer    │  ←结果──    │  └ PluginRegistry(子集) │
│  └ engine runtime        │             └──────────────────────────┘
└──────────────────────────┘             (启动时向 controller 注册)

形态 full (= controller + 本地 node 同进程):
┌──────────────── shirohad ────────────────┐
│ gRPC server (控制面) [controller 部分]    │
│  ├ ShirohaControl impl / TaskManager / ...│
│  └ engine runtime                        │
│ NodeExecutor [本地 node 部分]             │
│  ├ 启动时注册到本进程 controller          │
│  └ do-activity 可本地执行(不经网络)      │
└──────────────────────────────────────────┘
```

**外部接入不变：**
```
sctl (CLI, clap)                     Web/GUI (第三方, 非框架直接关联)
   │ gRPC client (tonic)                │ gRPC-gateway / HTTP shim (自选)
   ▼                                    ▼
┌──────────── shirohad (任一形态) ──────────┐
```

### 12.2 协议：gRPC（tonic）

选 gRPC 作为控制面主协议：
- tonic + prost + tonic-prost-build 已选型（Cargo.toml）。
- proto 强类型契约，跨语言（未来 GUI/Web 任意语言可接入）。
- 支持双向 streaming（task 状态观察 / OpenTelemetry 事件流 / node 结果流回传）。
- Web/GUI 不直接耦合 engine，通过 gRPC（或其自选的 gateway）接入控制面。

**两个 gRPC service：**
- `ShirohaControl`（controller 暴露，sctl/Web/GUI 消费）：task 生命周期 + 定义管理 + 观察流。
- `NodeExecutor`（node 暴露，controller 消费）：接收分发的 do-activity 执行请求，返回结果。controller 作为 `NodeExecutor` 的 gRPC client。

### 12.3 两层安全边界（MVP 均 no-op，留接缝）

| 层 | 位置 | 语义 | MVP |
|---|---|---|---|
| 传输层 auth | gRPC interceptor（控制面入口） | 「你是谁」——token/cert 鉴权调用方 | no-op interceptor（放行） |
| capability authz | task 创建边界（Authorizer trait, D5） | 「这个 task 能做什么」——capability 校验 | no-op Authorizer（放行） |

两层职责不同：传输层 auth 鉴别**调用方身份**，capability authz 校验**task 的能力集**。两者都不绕过对方。

### 12.4 控制面 gRPC service 草图（`proto/shiroha.proto`）

```proto
service ShirohaControl {
  // 定义管理
  rpc LoadDefinition(LoadDefinitionRequest) returns (LoadDefinitionResponse);
  rpc ListDefinitions(Empty) returns (ListDefinitionsResponse);

  // Task 生命周期
  rpc CreateTask(CreateTaskRequest) returns (CreateTaskResponse);   // 经 Authorizer
  rpc SendEvent(SendEventRequest) returns (SendEventResponse);
  rpc GetTaskState(GetTaskStateRequest) returns (TaskStateResponse);
  rpc ListTasks(Empty) returns (ListTasksResponse);
  rpc ControlTask(ControlTaskRequest) returns (ControlTaskResponse); // pause/resume/cancel

  // 观察流 (OpenTelemetry)
  rpc StreamTaskEvents(StreamTaskEventsRequest) returns (stream TaskEvent);
}

message CreateTaskRequest {
  string definition_id = 1;
  bytes  initial_context = 2;        // 初始上下文 (MVP bytes, 未来结构化)
  repeated string capabilities = 3;   // 申请的 capability (未来用, MVP 忽略)
}
message CreateTaskResponse { string task_id = 1; }

message SendEventRequest {
  string task_id = 1;
  string event = 2;
  bytes  payload = 3;
}
message SendEventResponse { bool accepted = 1; }

message TaskStateResponse {
  string task_id = 1;
  string current_state = 2;
  // 未来: history, active do-activity 等
}

message ControlTaskRequest {
  string task_id = 1;
  enum Action { PAUSE = 0; RESUME = 1; CANCEL = 2; }
  Action action = 2;
}
```

### 12.5 调用链（控制面如何消费 engine trait 接缝）

```
gRPC handler (ShirohaControl impl)
  ├── LoadDefinition  → Adapter::load() → IR → 缓存 definition_id
  ├── CreateTask       → Authorizer::authorize(req)? → TaskManager::create(ir, ctx) → TaskId
  ├── SendEvent        → TaskManager::handle(task_id)?.send(event)
  ├── GetTaskState     → TaskManager::state(task_id)
  ├── ControlTask      → TaskManager::control(task_id, action)  // pause/cancel
  └── StreamTaskEvents → TaskManager::event_stream(task_id)      // 观察流
```

**关键约束**：gRPC handler **不直接**操作 Task actor 内部状态，只通过 `TaskManager`（engine 暴露的控制面 struct）。`TaskManager` 是 engine 内 task 生命周期的唯一控制入口（持有 `TaskHandle` map）。

### 12.6 sctl 形态

```
sctl (clap CLI)
  ├── 子命令: definition load/list, task create/list/send/state/control, ...
  └── gRPC client (tonic) → shirohad:控制面
```

- sctl 是无状态 CLI，每次调用建短连接（或复用）调 shirohad。
- sctl 仅依赖 `shiroha-control` 的 client stubs + proto 类型，不拉 engine 运行时。

### 12.7 Web/GUI 边界

- Web/GUI 是第三方应用，**非框架直接关联**（父 PRD 已定）。
- 接入方式由 Web/GUI 自选：gRPC 直连、gRPC-gateway 转 HTTP、或自建 shim。
- 框架只保证控制面 gRPC service 稳定，不提供 Web/GUI 实现。

### 12.8 shirohad feature 矩阵与构建

**`shirohad` crate 的 feature 定义（`crates/shirohad/Cargo.toml` 伪）：**

```toml
[features]
default = ["full"]
full = ["controller", "node"]
controller = ["dep:shiroha-control", "dep:shiroha-engine", "dep:shiroha-wasm"]
node = ["dep:shiroha-control", "dep:shiroha-engine", "dep:shiroha-wasm"]  # node 侧 control 仅 client
```

- `full` = `controller` + `node`（默认），同进程跑 controller + 本地 node（node 注册到本进程 controller）。
- `controller` = 仅控制端（无本地 node，do-activity 分发到远端 node 或本地直接执行）。
- `node` = 仅节点（无控制面，启动时向指定 controller 注册，暴露 `NodeExecutor` service）。
- sctl 不受此 feature 影响（始终是 gRPC client）。

**构建命令（justfile 扩展）：**

```just
build-shirohad-full:    cargo build -p shirohad --features full
build-shirohad-ctrl:    cargo build -p shirohad --no-default-features --features controller
build-shirohad-node:    cargo build -p shirohad --no-default-features --features node
```

**形态选择与运行：**

| 形态 | 构建产物 | 启动行为 |
|---|---|---|
| full | `shirohad`（默认） | 起 gRPC server（控制面）+ 本地 node 注册到自身；do-activity 可本地直执行（不经网络）或经 NodeExecutor loopback |
| controller | `shirohad --no-default-features --features controller` | 只起控制面；do-activity 分发到已注册的远端 node |
| node | `shirohad --no-default-features --features node -- --controller <addr>` | 向 controller 注册 + 起 NodeExecutor service 待命 |

### 12.9 NodeExecutor gRPC service（`proto/shiroha.proto` 扩展）

```proto
service NodeExecutor {
  // node 生命周期
  rpc RegisterNode(RegisterNodeRequest) returns (RegisterNodeResponse);   // node→controller 注册

  // do-activity 执行
  rpc ExecuteActivity(ExecuteActivityRequest) returns (ExecuteActivityResponse);       // controller→node 分发
  rpc StreamActivity(stream ExecuteActivityRequest) returns (stream ExecuteActivityResponse); // 批量/stream
}

message RegisterNodeRequest {
  string node_id = 1;
  string listen_addr = 2;            // node 的 NodeExecutor 监听地址
  repeated string capabilities = 3;  // node 能提供的 capability (未来, MVP 忽略)
}
message RegisterNodeResponse { bool accepted = 1; }

message ExecuteActivityRequest {
  string task_id = 1;
  string activity = 2;               // do-activity 名称
  bytes  context = 3;                // ActionContext 序列化
}
message ExecuteActivityResponse {
  oneof result {
    ActionResult ok = 1;
    string error = 2;
  }
}
```

**controller 侧调度逻辑（MVP 极简）：**
- controller 维护已注册 node 列表。
- do-activity 需要分发时：若 full 形态且有本地 node，优先本地直执行（不经网络，loopback 调本地 ActionInvoker）；否则 round-robin 选一个已注册远端 node，经 `NodeExecutor::ExecuteActivity` 分发。
- MVP 调度策略极简（round-robin + 本地优先），复杂聚合策略走 `AggregationStrategy` plugin 能力面（§8，留口）。

**关键约束：**
- node 是**无状态**执行端：不持有 task 状态机，只接收 `ExecuteActivityRequest` 执行并返回结果。task 状态机始终在 controller 侧。
- node 的 `ActionInvoker`（WASM）与 controller 的 `ActionInvoker`（WASM）是同一实现（`shiroha-wasm`），只是调用方不同。
- `Transport` plugin 能力面（§8）未来可替换 node↔controller 的通信载体（rpc→p2p/消息服务），但 MVP 固定 gRPC。
