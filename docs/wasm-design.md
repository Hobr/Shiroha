# WASM 层设计

## Flow WASM — 用户状态机

用户编写的状态机逻辑，编译为单个 `.wasm` 文件。通过 WIT 接口与框架交互。

### Manifest 导出接口

WASM 模块导出 `get-manifest` 函数，返回整个状态机的拓扑描述。Controller 在部署 Flow 时调用一次并缓存，后续调度纯内存操作。

**FlowManifest 结构：**

| 字段 | 说明 |
| ------ | ------ |
| `host-world` | guest 声明的 capability world：`sandbox` / `network` / `storage` / `full` |
| `states` | 状态列表 |
| `transitions` | 转移列表 |
| `initial-state` | 初始状态名 |
| `actions` | Action 元信息注册表（含分发策略） |

**StateDef 结构：**

| 字段 | 说明 |
| ------ | ------ |
| `name` | 状态名 |
| `kind` | 状态类型：`normal` / `terminal` / `fork` / `join` / `subprocess` |
| `on-enter` | 可选，进入状态时执行的 action 函数名。当前 standalone 已真正执行 |
| `on-exit` | 可选，离开状态时执行的 action 函数名。当前 standalone 已真正执行 |
| `subprocess` | 可选，kind=subprocess 时必填。包含 `flow-id`（子 Flow ID）和 `completion-event`（子 Flow 完成后触发的事件） |

**TransitionDef 结构：**

| 字段 | 说明 |
| ------ | ------ |
| `from` | 源状态 |
| `to` | 目标状态 |
| `event` | 触发事件名 |
| `guard` | 可选，guard 函数名 |
| `action` | 可选，转移时执行的 action 函数名 |
| `timeout` | 可选，超时配置。包含 `duration-ms`（超时时长）和 `timeout-event`（超时后自动触发的事件名） |

**ActionDef 结构：**

| 字段 | 说明 |
| ------ | ------ |
| `name` | Action 函数名 |
| `dispatch` | 分发模式（见下文） |

### 分发模式

每个 Action 在 manifest 中声明分发模式，由用户决定哪些 action 需要被分发执行：

| 模式 | 说明 |
| ------ | ------ |
| `local` | Controller 本地执行，适合轻量操作 |
| `remote` | 分发到单个 Node 执行；当前 standalone 会退化为同进程内执行 |
| `fan-out` | 分发到多个 Node 并行执行，聚合结果后决定状态转移 |

**Fan-out 配置：**

| 字段 | 说明 |
| ------ | ------ |
| `strategy` | 分发策略：`all`（所有 Node）/ `count(N)`（N 个 Node）/ `tagged(标签列表)`（带指定标签的 Node） |
| `aggregator` | 聚合函数名（WASM 内定义） |
| `timeout-ms` | 可选，fan-out 整体超时 |
| `min-success` | 可选，最少成功数，达到即可提前聚合 |

### 执行接口

WASM 模块导出以下执行接口供框架调用：

| 接口 | 调用方 | 说明 |
| ------ | -------- | ------ |
| `invoke-action(name, context)` | Controller / Node | 执行指定 Action，传入 job-id、当前状态、持久化 job context、事件 payload；返回执行状态和输出 |
| `invoke-guard(name, context)` | Controller | 评估 Guard 条件，传入 job-id、from/to 状态、事件、持久化 job context、payload；返回 bool |
| `aggregate(name, results)` | Controller | 聚合多 Node 的执行结果（含 node-id、状态、输出），返回决策事件和可选的 context 补丁 |

执行状态包含：`success` / `failed` / `timeout`。

聚合决策返回一个事件名，Controller 用该事件驱动状态机转移。

### Component / wasip2 ABI

当前实现仅支持 component model 路线：

- 不需要额外能力的 guest 实现 `crate/shiroha-wit/wit/flow.wit` 中定义的 `world flow`
- 需要 HTTP 的 guest 实现 `crate/shiroha-wit/wit/network-flow.wit` 中定义的 `world network-flow`
- 需要 KV 存储的 guest 实现 `crate/shiroha-wit/wit/storage-flow.wit` 中定义的 `world storage-flow`
- 同时需要 HTTP + KV 存储的 guest 实现 `crate/shiroha-wit/wit/full-flow.wit` 中定义的 `world full-flow`
- HTTP capability 类型定义位于 `crate/shiroha-wit/wit/net.wit`
- KV capability 类型定义位于 `crate/shiroha-wit/wit/store.wit`
- host 使用 `wasmtime::component` typed exports 调用 `get-manifest` / `invoke-action` / `invoke-guard` / `aggregate`
- component 实例化时接入 `wasmtime_wasi::p2`，因此 guest 应编译为 `wasm32-wasip2`
- 上传的二进制必须是合法 component；core module 已不再接受
- 部署时框架会校验 manifest 声明的 `host-world` 与组件实际 imports 是否一致
- 当前 deploy 只验证通用导出接口是否存在，不会逐个验证 manifest 中命名的 `action` / `guard` / `aggregator` 在 guest 内部分支里是否真正实现

### 执行流程

**普通 Action（local / remote）：**

```
Controller                     Node
    │  状态转移触发 action         │
    │  查 manifest → dispatch=remote
    │──── invoke-action ──────────►│
    │◄─── action-result ──────────│
    │  用结果继续推进状态机         │
```

当前 standalone 实现里，`local` 和 `remote` 都由同进程内的 host 直接调用 guest typed export，区别只保留在 manifest 语义层；还没有一个真实的 in-process Controller↔Node 执行边界。

当前限制：

- action 返回的 `output` 当前只用于 guest / host 调试与测试，不会进入事件日志，也不会自动反馈到后续流程上下文

### Host Network Import

当前 `world network-flow` 已提供 `net.send(client, request)` host import：

- guest 可按请求传入 `client-config` 和 `request-options`
- `client-config` 当前支持 default headers、user-agent、timeout、connect timeout、pool、TCP、redirect、proxy、cookie store、compression、TLS 版本、root cert、https-only、本地地址等配置
- `request-options` 当前支持 method、url、headers、query、HTTP version、per-request timeout、bearer/basic auth、body、`error-for-status`
- host 使用 reqwest 执行请求，并把 status / url / version / headers / body 返回给 guest

### Host Storage Import

当前 `world storage-flow` / `world full-flow` 已提供 `store` host import：

- `store.get(namespace, key)`：读取字节值
- `store.put(namespace, key, value)`：写入字节值
- `store.delete(namespace, key)`：删除键并返回是否存在
- `store.list-keys(namespace, prefix, limit)`：列出命名空间内的键

当前实现状态：

- `shirohad` 运行时里，`store` 已接到真实的 Shiroha 存储后端
- standalone 路径下，组件通过 `store` 写入的数据会跟随服务端数据目录一起保留，并可在重启后继续读取
- 独立的 `shiroha-wasm` host 单元测试里，`WasmHost::new()` 仍会退化到内存 store，方便不依赖 `shirohad` 单独测试 capability

当前限制：

- 目前已经有 `flow` / `network-flow` / `storage-flow` / `full-flow` 这些 world，并且 deploy 会校验声明 world 与 imports 一致
- 当前运行时会按 `action.capabilities` 在每次调用时动态放行 network/storage；因此已经存在 action 级 capability gating
- 但 `storage` capability 仍没有 flow/job 级 namespace 隔离，拿到权限的 guest 仍可访问任意 namespace

**Fan-out Action：**

```
Controller                          Nodes
    │                                 │
    │  1. 查 manifest → fan-out(all)  │
    │                                 │
    │──── invoke-action ─────────────►│ Node A
    │──── invoke-action ─────────────►│ Node B
    │──── invoke-action ─────────────►│ Node C
    │                                 │
    │  2. 收集结果（或达到 min-success）
    │◄─── action-result ─────────────│
    │◄─── action-result ─────────────│
    │◄─── action-result ─────────────│
    │                                 │
    │  3. 调用 WASM 聚合函数          │
    │     → 返回决策事件              │
    │                                 │
    │  4. 用返回的事件驱动状态转移     │
```

当前实现状态：

- `fan-out` manifest / guest ABI / aggregate host 调用已经打通
- manifest 当前仍可通过 deploy，但 standalone 运行时一旦真正执行到 `fan-out` action 会直接返回 `unimplemented`
- standalone 运行时尚未真正执行多节点 fan-out 调度与聚合
- `subprocess` manifest 声明已可部署，但自动父子 Job 编排尚未实现

## Plugin WASM — 框架扩展

框架级逻辑也可通过 WASM 插件替换。与 Flow WASM 是独立的模块。

### 适合做 WASM 插件的

| 插件类型 | 说明 |
| ---------- | ------ |
| 调度算法 | round-robin、最小负载、加权、自定义 |
| Fan-out 策略 | 自定义分组/过滤逻辑 |
| 路由/过滤 | 根据任务特征选择 Node 子集 |
| 中间件/Hook | Action 执行前后的拦截（限流、数据变换） |

这些都是纯计算、无 I/O、调用频率相对低的逻辑，WASM 开销可忽略。

### 不适合做 WASM 插件的

| 模块 | 原因 |
| ------ | ------ |
| Storage 后端 | 重 I/O，需直接操作文件/网络/数据库连接池 |
| Transport 层 | 需要 OS socket、TLS、连接管理 |
| WASM Runtime | 自举悖论 |
| 核心状态机引擎 | 正确性命脉，不应交给用户代码 |
| 健康检查/心跳 | 基础设施，必须可靠 |

这些通过 Rust trait 实现插拔，编译时或配置时选择。

### 插件接口

**调度插件**：接收调度请求（action 名称、分发模式、可用 Node 列表及其标签/负载/活跃任务数、上下文），返回选中的 Node ID 列表。

**中间件插件**：

- `before-action`：Action 执行前调用，可选择放行、拒绝（附原因）、或修改上下文后继续
- `after-action`：Action 执行后调用，可修改返回结果

### 加载优先级

用户 WASM 插件 > 框架内置 Native Rust 实现。

Controller 启动时检查每个插件槽位（scheduler、middleware 等）是否有用户提供的 WASM 插件。有则加载，无则使用内置的 Rust 默认实现。框架零配置即可运行。

## 权限系统

基于 WIT world 的分层能力控制：

| World | 能力 | 适用场景 |
| ------- | ------ | ---------- |
| `sandbox` | 纯计算，无 I/O | Guard、确定性 Action、Replicated 模式（强制） |
| `network` | HTTP 客户端 | 需要调用外部 API 的 Action |
| `storage` | kv-store / blob-store | 需要持久化数据的 Action |
| `full` | 所有能力 | 需显式授权 |

设计上的运行时限制目标：

- `fuel`：执行步数上限
- `memory_mb`：WASM 线性内存上限
- `timeout_ms`：单次执行超时
- `max_concurrent`：并发实例上限

当前实现状态：

- host 已启用 fuel，并为每次 guest 调用设置固定预算
- `memory_mb` / `timeout_ms` / `max_concurrent` 还没有用户可配置的运行时控制面

框架实例化 WASM 时根据声明的 world 只链接对应的 host function，未授权调用触发 trap。

## WASM 模块管理

- **进程内缓存**：运行时按 content hash 缓存编译后的 component，避免重复编译
- **版本管理**：Job 创建时绑定 Flow 版本，多版本 WASM 可共存
- **持久化恢复**：Controller 会持久化原始 WASM 字节，重启后重建模块缓存，继续执行旧 Job
- **Node 端缓存**：Controller 维护模块 registry，Node 按 content hash 缓存，任务只下发模块 ID + 函数入口（分布式阶段）
- **确定性保证**：NaN 规范化 + Guard 强制 sandbox world
