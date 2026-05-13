# 节点间传输层 (shiroha-transport / shiroha-transport-grpc)

## 角色

`shiroha-transport` 定义节点间 RPC 抽象。`shiroha-transport-grpc` 是它的第一个具体实现。未来 QUIC、消息队列等实现各起一个 crate,统一实现 transport trait。

注意:本层只负责**节点面通信**(主控 ↔ worker),不负责控制面通信(sctl ↔ shirohad)。后者见 `control-plane.md`。

## 抽象边界

Transport 至少需要回答以下问题:

- 把一份 ActionRef + 输入字节投递到指定节点,等待结果字节
- 在投递中途取消尚未完成的请求(服务 Aggregator 的提前返回语义)
- 检测节点健康(心跳/连通性),并在状态变化时通知上层
- 在节点列表变化时通知 NodeRegistry

Transport **不**负责:

- 决策派给哪些节点(是 Dispatcher 的职责)
- 解析 Action 含义(是 wasm 的职责)
- 缓存 WASM 组件字节(取决于部署模式,见 `open-questions.md`)
- 聚合结果(是 Aggregator 的职责)

## 节点注册表 (NodeRegistry)

NodeRegistry 是 Dispatcher 与 Transport 之间的查询面。它提供:

- 列出全部已注册节点
- 按节点选择器筛选(标签、地域、能力等)
- 健康状态查询

注册模型(静态配置 vs 动态注册 vs 混合)见 `open-questions.md`。

## gRPC 实现 (shiroha-transport-grpc)

- 服务定义放在 `shiroha-proto`,以保持 build.rs 与 tonic 代码生成集中
- 客户端连接池由 transport-grpc 自管;Dispatcher 看到的只是 "submit / cancel" 这两个动作
- 流式响应支持(在 Action 长执行时返回进度)在初期可不实现,但服务定义要预留 streaming RPC 入口,避免日后破坏性变更
- 鉴权:MVP 阶段在受信网络内运行,远程链路 mTLS 与令牌策略列入后续工作

## 跨实现的不变量

任何新实现 (QUIC / NATS / SQS / etc.) 必须满足:

- 不引入额外的状态机概念
- 错误分两类:网络错误(可由上层选择重试)与对端业务错误(透传给 Dispatcher 由 Aggregator 处理)
- 必须支持取消语义(允许实现为 best-effort,但调用方需要被告知"已发出取消")
- 必须支持健康检查或等价机制,使 NodeRegistry 能在不调用业务接口的前提下判定可用性

## 拓扑约束

- 节点之间不互相通信;一切协作经主控
- 主控持有所有出站节点连接;节点不主动连向其他节点
- 节点上报心跳给主控,主控聚合后维护 NodeRegistry

这一约束是"主控集中状态"原则的直接推论;打破它会让节点之间出现局部状态,违背架构基本假设。

## 与其他 crate 的契约

- 入参 / 出参:与 Dispatcher 之间用 ActionRef + 字节
- 依赖:`shiroha-core` 的 NodeId、NodeSelector 等基础类型
- 被依赖:`shiroha-dispatch`、`shiroha-worker`(节点侧使用 transport 接收请求)
