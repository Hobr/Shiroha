# gRPC 服务划分

主控暴露两个服务，边界按调用方划分而非按资源划分，避免客户端与节点混用同一接口。

## OrchestratorService (用户侧)

面向客户端 (CLI / 未来的 TUI / Web)：

- **DeployFsm**：上传 WASM，返回 FSM 标识 (内容哈希)。
- **CreateInstance**：基于已部署的 FSM 创建实例并给出初始上下文。
- **SubmitEvent**：向实例提交事件，触发状态迁移。
- **QueryState**：读取实例当前状态。
- **StreamEvents**：订阅实例的事件/状态流。

## NodeService (节点侧)

面向 Agent：

- **Register**：节点上线时登记能力、标签、地址。
- **Heartbeat**：周期心跳；超时视为下线。
- **FetchWasm**：按哈希下载 WASM 模块。
- **ReportResult** (可选)：若采用反向推送式结果回传。

## 节点入口

节点侧反向监听一个 `Invoke` 入口，接受 `{wasm_hash, action, input}` 并返回结果。此入口由 `shiroha-transport-grpc` 封装，节点无需感知 gRPC 细节。

## 协议演进

- Proto 文件集中于 `proto/`，由 `shiroha-proto` 统一生成 stub。
- 破坏性变更走显式版本；字段新增优先。
- 后续新传输 (Quic / MQ) 仅替换 `Transport` 实现，服务定义保持不变。
