# Plugin Architecture

## Overview

Shiroha 的 plugin 架构是框架级扩展点系统，设计为**单注册表、多能力面**模型：一个 plugin 可注册一项或多项能力（ActionFunc、Middleware、AggregationStrategy、Transport、Adapter），运行时通过 `PluginRegistry` 统一查找和路由。

核心设计目标：
- **类型安全的扩展点**：每个能力面是独立 trait，plugin 按需实现
- **两层语义**：plugin type name 定位能力实现 → action instance name 传递配置
- **immutable-after-init**：registry 初始化后不可变，避免运行时锁竞争

## Two-Layer ActionRef Semantics

Plugin 系统采用**两层语义**分离 plugin 类型与 action 实例：

```rust
// shiroha-ir/src/action.rs
pub struct ActionRef {
    pub plugin: String,  // plugin type name, e.g. "http" or "wasm"
    pub name: String,    // action instance name, e.g. "fetch-user-api"
}
```

### 层次分工

1. **第一层：plugin type name → ActionFunc lookup**
   - `plugin` 字段定位 plugin 实现（如 `"http"` → `HttpActionFunc`）
   - `PluginRegistry` 按 plugin type name 查找能力面实现
   - 一个 plugin type 只注册一次

2. **第二层：action instance name → payload config**
   - `name` 字段标识 action 实例（如 `"fetch-user-api"`）
   - payload（JSON/TOML）携带该实例的配置（URL、headers、超时等）
   - 同一 plugin type 可有多个 action 实例（不同 name + 不同 payload）

### 为什么需要两层

单层设计（只有 action name）会导致：
- ❌ 无法区分 `"log"` 是 WASM action 还是 framework action
- ❌ plugin 查找需要遍历所有注册的 action name（O(n) 查找）
- ❌ action 实例配置耦合到 plugin 注册逻辑

两层设计解决这些问题：
- ✅ plugin type name 是稳定的路由键（`"http"` / `"wasm"` / `"bash"`）
- ✅ O(1) HashMap lookup：`registry.actions.get(&action_ref.plugin)`
- ✅ action 实例配置独立存储（状态机定义内，不在 plugin 代码内）

### 实例

状态机定义（WIT 或 JSON）：
```wit
// WASM component 导出
export actions: func() -> list<action-def> {
  [
    { name: "fetch-user", plugin: "http", payload: "{\"url\": \"https://api.example.com/user\"}" },
    { name: "log-entry", plugin: "wasm", payload: "{\"component\": \"logger.wasm\"}" },
  ]
}
```

运行时路由：
```rust
// CompositeActionInvoker::invoke_sync
let action_ref = ActionRef { plugin: "http", name: "fetch-user" };

// 第一层：查找 plugin type "http" → HttpActionFunc
let func = registry.actions.get(&action_ref.plugin)?;

// 第二层：传递 action instance name + payload 到 ActionFunc::invoke
func.invoke(&action_ref.name, payload, ctx).await?;
```

## PluginRegistry Pattern

### Immutable-After-Init 模式

```rust
// shiroha-plugin/src/registry.rs
pub struct PluginRegistry {
    // 每个能力面用独立 HashMap，按 plugin type name 索引
    actions: Arc<HashMap<String, Arc<dyn ActionFunc>>>,
    middleware: Arc<HashMap<String, Arc<dyn Middleware>>>,
    transports: Arc<HashMap<String, Arc<dyn Transport>>>,
    // ... 其他能力面
}

impl PluginRegistry {
    pub fn builder() -> RegistryBuilder { /* ... */ }

    // 只读查找，无锁
    pub fn get_action(&self, plugin_type: &str) -> Option<Arc<dyn ActionFunc>> {
        self.actions.get(plugin_type).cloned()
    }
}
```

### 为什么选择 Arc without locks

**Alternative 1: Arc<RwLock<HashMap>>**
- ❌ 每次查找需要 `read()` 获取锁（即使只读）
- ❌ 高并发下锁竞争成为瓶颈（每个 task 实例都查找 action）
- ❌ 不必要的复杂性：registry 初始化后不再修改

**Alternative 2: 全局静态 HashMap**
- ❌ 无法支持多 registry 实例（测试场景需要隔离）
- ❌ 初始化顺序问题（static 初始化时机不明确）

**Chosen: Arc<HashMap> immutable-after-init**
- ✅ 零锁开销：查找是纯读操作，Arc 保证线程安全
- ✅ 写一次（builder 构建） → 多次读（运行时查找）
- ✅ 支持多实例：每个 Engine 可有独立 registry（测试隔离）
- ✅ 清晰的生命周期：builder → build() → immutable registry

### 初始化模式

```rust
// shirohad 启动时
let registry = PluginRegistry::builder()
    .register_action("http", Arc::new(HttpActionFunc::new()))
    .register_action("wasm", Arc::new(WasmActionFunc::new(engine)))
    .register_middleware("auth", Arc::new(AuthMiddleware::new()))
    .build(); // 构建后不可变

// Engine 持有 registry Arc
let engine = Engine::new(registry);

// 多个 task 并发查找，无锁
let func = engine.registry.get_action("http")?;
```

## ActionKind Evolution

### v0.2.0: 枚举值

```rust
pub enum ActionKind {
    Wasm,
    Plugin,
}
```

问题：`ActionKind::Plugin` 无法标识具体 plugin type（http / bash / ...）

### v0.2.5: 携带实现名称

```rust
// shiroha-ir/src/action.rs
pub enum ActionKind {
    Wasm(String),   // String = component path or module name
    Plugin(String), // String = plugin type name (e.g. "http")
}
```

IR 层存储 plugin type name：
```rust
pub struct ActionDef {
    pub name: String,           // action instance name
    pub kind: ActionKind,       // Plugin("http") or Wasm("counter.wasm")
    pub payload: Option<String>,
}
```

转换为运行时 `ActionRef`：
```rust
// WasmAdapter::convert_action_def
let action_ref = match &def.kind {
    ActionKind::Plugin(plugin_type) => ActionRef {
        plugin: plugin_type.clone(),
        name: def.name.clone(),
    },
    ActionKind::Wasm(component) => ActionRef {
        plugin: "wasm".to_string(),
        name: def.name.clone(),
    },
};
```

### 为什么演进

- **类型安全**：`ActionKind::Plugin(String)` 强制 adapter 提供 plugin type name
- **可扩展**：新增 plugin type 不需修改 `ActionKind` enum（只需注册新 plugin）
- **向前兼容**：未来可添加 `ActionKind::Native(fn_pointer)` 支持原生 Rust action

## Extension Points

Shiroha 定义五个能力面 trait，plugin 按需实现：

### 1. ActionFunc

```rust
#[async_trait]
pub trait ActionFunc: Send + Sync {
    async fn invoke(
        &self,
        name: &str,        // action instance name
        payload: &str,     // JSON/TOML config
        ctx: &ActionContext,
    ) -> Result<ActionResult>;
}
```

用途：
- entry/exit/transition action 执行
- 同步副作用（fire-and-forget）或 async do-activity
- 例：HTTP request、bash 命令、WASM 函数调用

### 2. Middleware

```rust
#[async_trait]
pub trait Middleware: Send + Sync {
    async fn process(
        &self,
        event: Event,
        next: &dyn Fn(Event) -> Pin<Box<dyn Future<Output = Result<()>>>>,
    ) -> Result<()>;
}
```

用途：
- 事件处理前后插入逻辑（认证、日志、监控）
- 拦截或修改事件
- 例：auth middleware、rate limiter、audit logger

### 3. AggregationStrategy（第二层）

```rust
#[async_trait]
pub trait AggregationStrategy: Send + Sync {
    async fn aggregate(
        &self,
        results: Vec<TaskResult>,
    ) -> Result<AggregatedResult>;
}
```

用途：
- 分布式调度器收集多个 node 执行结果
- MapReduce 风格聚合
- 例：majority vote、first-success、all-or-nothing

### 4. Transport（第二层）

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, dest: &NodeId, msg: Message) -> Result<()>;
    async fn recv(&self) -> Result<Message>;
}
```

用途：
- controller ↔ node 通信
- 例：gRPC、message queue（NATS/Kafka）、P2P（libp2p）

### 5. Adapter

```rust
#[async_trait]
pub trait Adapter: Send + Sync {
    async fn load(&self, source: &str) -> Result<StateMachineDef>;
}
```

用途：
- 扩展状态机定义来源
- 例：WASM component（已实现）、JSON/TOML 文件、远程 API、数据库

## Implementation Status

### v0.2.5 (已完成)
- ✅ `ActionRef` 两层语义实现
- ✅ `ActionKind` 携带实现名称
- ✅ `WasmActionInvoker` 完整实现（真实 WASM 执行）
- ✅ `CompositeActionInvoker` 路由框架（Wasm/Plugin 分支）

### v0.3.0 (计划中)
- [ ] `PluginRegistry` + `Plugin` trait
- [ ] 五个能力面 trait 定义（**全部为 stub**，仅定义接口）
- [ ] `CompositeActionInvoker` 修改为传递 `ActionRef`（当前传递 `&str`）
- [ ] 单元测试：空 registry 查找、路由逻辑

### v0.3.5+ (未排期)
- [ ] `HttpActionFunc` 实现（第一个真实 plugin）
- [ ] `BashActionFunc` 实现
- [ ] `AuthMiddleware` 实现
- [ ] plugin 配置系统（从 TOML 加载 plugin 列表）

## Examples

### 注册 plugin

```rust
// shirohad/src/main.rs
let registry = PluginRegistry::builder()
    .register_action("http", Arc::new(HttpActionFunc::new(
        reqwest::Client::new(),
    )))
    .register_action("wasm", Arc::new(WasmActionInvoker::new(
        wasmtime::Engine::default(),
    )))
    .register_middleware("auth", Arc::new(AuthMiddleware::new(
        "/etc/shirohad/auth.toml",
    )))
    .build();
```

### 定义 action（状态机 WIT）

```wit
// state-machine.wit
export actions: func() -> list<action-def> {
  [
    {
      name: "notify-user",
      plugin: "http",
      payload: "{\"method\": \"POST\", \"url\": \"https://api.example.com/notify\"}"
    },
    {
      name: "log-state-change",
      plugin: "wasm",
      payload: "{\"component\": \"logger.wasm\", \"function\": \"log\"}"
    },
  ]
}
```

### 路由执行

```rust
// CompositeActionInvoker::invoke_sync
pub async fn invoke_sync(&self, action_ref: &ActionRef) -> Result<()> {
    match action_ref.plugin.as_str() {
        "wasm" => self.wasm_invoker.invoke(action_ref).await,
        plugin_type => {
            let func = self.registry.get_action(plugin_type)
                .ok_or_else(|| Error::PluginNotFound(plugin_type.to_string()))?;
            func.invoke(&action_ref.name, &action_ref.payload, &self.ctx).await
        }
    }
}
```

## Trade-offs

### 为什么不用动态加载（`.so` / `.dylib`）

**Rejected alternative**: plugin 为动态库，运行时 `dlopen()` 加载

优点：
- 可独立编译 plugin
- 无需重新编译 shirohad

缺点：
- ❌ ABI 不稳定：Rust 无稳定 ABI，plugin 必须用相同 rustc 版本编译
- ❌ 版本地狱：plugin API 变更导致所有 `.so` 需重新编译
- ❌ 安全风险：动态库可执行任意代码，难以沙箱隔离
- ❌ 类型安全丢失：`dlsym()` 返回 `*mut c_void`，需手动转换

**Chosen**: 静态链接 trait-based plugin

优点：
- ✅ 类型安全：编译期检查 trait 实现
- ✅ 零运行时开销：单态化后无虚表查找
- ✅ 简单：plugin 只是实现 trait 的 struct

缺点：
- 新增 plugin 需重新编译 shirohad（可接受：plugin 数量有限，变更不频繁）

### 为什么不用 `Any` + downcast

**Rejected alternative**: `HashMap<String, Arc<dyn Any>>`，运行时 downcast

```rust
let func = registry.get("http")?
    .downcast::<HttpActionFunc>()?; // 运行时类型检查
```

缺点：
- ❌ 类型不安全：downcast 失败是运行时 panic
- ❌ 无法强制 plugin 实现特定 trait
- ❌ 难以静态分析（IDE 无法自动补全）

**Chosen**: 每个能力面独立 `HashMap<String, Arc<dyn Trait>>`

优点：
- ✅ 编译期类型检查：`Arc<dyn ActionFunc>` 强制实现 `ActionFunc`
- ✅ 清晰的能力面边界：actions / middleware / transports 独立查找
- ✅ IDE 友好：trait 方法自动补全

## Future Work

### Plugin 配置系统（v0.4.0+）

从配置文件加载 plugin：

```toml
# /etc/shirohad/plugins.toml
[plugins.http]
type = "http"
timeout = "30s"
max_redirects = 5

[plugins.auth]
type = "middleware"
provider = "jwt"
secret_key = "/etc/shirohad/jwt.key"
```

运行时解析并注册：
```rust
let config = PluginConfig::load("/etc/shirohad/plugins.toml")?;
let registry = PluginRegistry::from_config(config)?;
```

### WASM plugin（未定）

plugin 本身也可编译为 WASM：

```rust
// http-plugin.wasm 实现 ActionFunc trait
pub struct WasmPlugin {
    instance: wasmtime::Instance,
}

impl ActionFunc for WasmPlugin {
    async fn invoke(&self, name: &str, payload: &str, ctx: &ActionContext) -> Result<ActionResult> {
        let func = self.instance.get_typed_func::<(String, String), String>("invoke")?;
        let result = func.call_async((name.to_string(), payload.to_string())).await?;
        Ok(ActionResult::from_json(&result)?)
    }
}
```

优点：
- 沙箱隔离：plugin 无法访问宿主文件系统
- 跨平台：WASM plugin 可在任意平台运行

挑战：
- WASM 无法直接 async（需 wasmtime async 支持）
- host import 复杂度（plugin 需访问 registry / task context）

## References

- `shiroha-ir/src/action.rs` — `ActionRef` / `ActionKind` 定义
- `shiroha-plugin/src/registry.rs` — `PluginRegistry` 实现
- `shiroha-wasm/src/invoker.rs` — `WasmActionInvoker` 实例
- `shiroha-engine/src/invoker.rs` — `CompositeActionInvoker` 路由
- `.trellis/tasks/06-29-layer1-statemachine-wasm-adapter/design.md` §8 — plugin 架构设计
