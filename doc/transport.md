# 节点间传输层 (shiroha-transport / shiroha-transport-grpc)

## 角色

`shiroha-transport` 定义节点间 RPC 抽象。`shiroha-transport-grpc` 是它的第一个具体实现。未来 QUIC、消息队列等实现各起一个 crate,统一实现 transport trait。

注意:本层只负责**节点面通信**(主控 ↔ worker),不负责控制面通信(sctl ↔ shirohad)。后者见 `control-plane.md`。

## 抽象边界

Transport 需要支持以下 RPC:

**主控 → 节点**:

- **submit-action** — 把 `(ComponentId, ActionRef, 输入字节)` 投递到指定节点,等待结果字节
- **cancel-action** — 取消尚未完成的请求(服务 Aggregator 的提前返回语义)

**节点 → 主控**:

- **fetch-component** — 按 `ComponentId` 拉取 WASM 字节(服务节点按需 pull,见 `worker.md`)
- **heartbeat** — 周期上报心跳

此外:

- 检测节点健康(心跳/连通性),并在状态变化时通知上层
- 在节点列表变化时通知 NodeRegistry

Transport **不**负责:

- 决策派给哪些节点(是 Dispatcher 的职责)
- 解析 Action 含义(是 wasm 的职责)
- 缓存 WASM 组件字节(由 worker 自行维护,详见 `worker.md`)
- 聚合结果(是 Aggregator 的职责)

### 在途 Action 与节点离线的竞态

节点的心跳通道与 Action 结果通道是独立的。当心跳超时导致节点被标记为不可用时,已在该节点上派发但尚未返回结果的 Action 仍应被正常接收——Transport 不应因节点标记为不可用而丢弃已到达的结果。若 Aggregator 已因超时提前返回,晚到的结果进入 dead letter(见 `dispatch.md`);若 Aggregator 仍在等待,结果正常参与聚合。

## 节点注册表 (NodeRegistry)

NodeRegistry 是 Dispatcher 与 Transport 之间的查询面。它提供:

- 列出全部已注册节点
- 按节点选择器筛选(标签、地域、能力等)
- 健康状态查询

注册模型分两阶段演进:

- **MVP 阶段:静态配置** — 主控启动时从配置文件加载节点 endpoint 与能力标签;新增节点需热重载配置(或主控重启)
- **后续阶段:混合模型** — 配置预声明节点身份与角色,节点启动时上报实际能力并确认存活;主控按心跳维护"已声明 ∩ 已连接 ∩ 健康"的有效节点集

NodeRegistry 的对外查询接口在两阶段间保持兼容,这样上层 Dispatcher 不感知演进。完全动态注册(节点自由加入)暂不考虑。

## gRPC 实现 (shiroha-transport-grpc)

- 服务定义放在 `shiroha-proto-node`,以保持 build.rs 与 tonic 代码生成集中
- 客户端连接池由 transport-grpc 自管;Dispatcher 看到的只是 "submit / cancel" 这两个动作
- 流式响应支持(在 Action 长执行时返回进度)在初期可不实现,但服务定义要预留 streaming RPC 入口,避免日后破坏性变更
- 鉴权:MVP 阶段在受信网络内运行,远程链路 mTLS 与令牌策略列入后续工作

## 跨实现的不变量

任何新实现 (QUIC / NATS / SQS / etc.) 必须满足:

- 不引入额外的状态机概念
- 错误分两类:网络错误(可由上层选择重试)与对端业务错误(透传给 Dispatcher 由 Aggregator 处理)
- 必须支持取消语义(允许实现为 best-effort);调用方需被告知"已发出取消";晚到的结果由 Dispatcher 处理(见 `dispatch.md`),transport 仅透传
- 必须支持健康检查或等价机制,使 NodeRegistry 能在不调用业务接口的前提下判定可用性

## 拓扑约束

- 节点之间不互相通信;一切协作经主控
- 主控持有所有出站节点连接;节点不主动连向其他节点
- 节点上报心跳给主控,主控聚合后维护 NodeRegistry

这一约束是"主控集中状态"原则的直接推论;打破它会让节点之间出现局部状态,违背架构基本假设。

## 与其他 crate 的契约

- 入参 / 出参:与 Dispatcher 之间 `(ComponentId, ActionRef, 输入字节) → 输出字节`;反向支持 worker 的 `ComponentId → 字节` 拉取
- 依赖:`shiroha-core` 的 NodeId、NodeSelector 等基础类型
- 被依赖:`shiroha-dispatch`、`shiroha-worker`(节点侧使用 transport 接收请求);不被 `shiroha-engine` 直接依赖(经由 dispatch)
