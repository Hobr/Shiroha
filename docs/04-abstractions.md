# 关键抽象

此处仅描述抽象的**职责、输入输出与不变式**，不涉及具体实现。

## StateMachineDef (shiroha-core)

- 只读视图：状态集合、合法转换、Action 元数据。
- Action 元数据中包含该 Action 的分发声明 (`DispatchSpec`) 与聚合声明 (`AggregateSpec`)。
- 纯数据结构；不涉及执行。

## Dispatcher (shiroha-dispatch)

- 输入：Action 元数据 + 运行时上下文 (已注册节点集、负载/标签等)。
- 输出：一个 `DispatchPlan`，描述本次执行要调用的节点集合与扇出方式。
- 内置变体：`Local` (共址)、`One` (选一)、`Many` (多播/分片/加权)。
- 不执行调用，只产出计划。

## Aggregator (shiroha-aggregate)

- 输入：节点执行结果的异步流。
- 输出：单一的 Action 输出，供状态机继续推进。
- 内置变体：`First` (首个成功)、`AllOk` (全部成功后合并)、`Quorum(n)` (达到法定数)、`Reduce` (由 WASM 自定义)。
- 在主控执行；自定义 reduce 复用 `shiroha-wasm` 的独立沙箱。

## Transport (shiroha-transport)

- 职责：把 Action 请求送达节点、把结果带回；以及让节点按哈希拉取 WASM 模块。
- 抽象层仅定义调用语义 (一应一答、流式结果、超时)；具体协议由各 `shiroha-transport-*` 实现。
- 节点发现以 trait 形式暴露，首版使用静态配置 + 注册 RPC，后续可接入外部注册中心。

## WasmRuntime (shiroha-wasm)

- 持有 Wasmtime Engine 与模块缓存；模块按内容哈希寻址。
- 两项能力：从 WASM 导出读取 FSM 定义；执行指定 Action。
- 承载 host 侧的 WIT 实现 (日志、KV、HTTP、时钟、随机、指标)。
- **沙箱资源约束默认启用**：fuel / epoch interruption 限制执行时间；每个 Store 有内存上限；WASI 能力按 FSM 声明白名单开放。详见 [security](11-security.md)。
- 不关心调度与分发。

## Controller (shiroha-controller)

- 汇聚上述组件；对外以两个 gRPC 服务呈现。
- 职责边界：实例生命周期、**事件串行队列**、调度协调、聚合编排、状态迁移与事件日志落库 (单事务)、超时调度器。
- 不承载 Action 的具体执行逻辑。

## Node (shiroha-node)

- 无状态：注册、心跳、按请求执行 Action。
- 不做业务决策；不保留跨请求的状态 (除 WASM 模块缓存)。

## 不变式

- 同一实例的事件严格串行处理；并发事件排队等待。详见 [semantics](05-semantics.md)。
- 状态迁移、事件日志、超时登记必须在同一事务内落地，否则整次事件回滚。
- Action 执行语义为 at-least-once；副作用幂等性由用户保证，框架提供 `attempt_id`。
- Action 的分发与聚合策略来自 FSM 定义，不允许运行时由调用方覆写 (预留策略替换接口留作后续议题)。
- 节点永远不直接访问主控的持久化层。
- `Dispatcher` / `Aggregator` 的 trait 契约须保持稳定，远期 WASM 自定义策略 (见 [roadmap](12-roadmap.md)) 将复用同一契约，不另立体系；冻结字段见 [contracts](09-contracts.md)。
