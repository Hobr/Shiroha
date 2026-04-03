# 可插拔后端（Native Trait）

## Transport

节点间通信，通过 trait 抽象。当前核心能力只有：向指定 Node 发送消息、广播。

| 后端 | 场景 | 实现优先级 |
| ------ | ------ | ----------- |
| In-Process (tokio mpsc) | standalone 模式、开发测试 | P0 |
| gRPC (tonic) | 生产集群、Controller↔Node | P1 |
| libp2p / QUIC | 去中心化 P2P、边缘计算 | P2 |
| NATS | 事件驱动、异步解耦 | P2 |

备注：

- Phase 1 真正可用的是 standalone 路径
- `Transport` trait 已定义，但分布式 transport 仍属于后续阶段

## Storage

状态持久化，通过 trait 抽象。

| 后端 | 场景 | 实现优先级 |
| ------ | ------ | ----------- |
| Memory | 开发测试 | P0 |
| Redb | 嵌入式生产环境 | P0 |
| SQLite | 单机生产、工具链友好 | P1 |
| PostgreSQL | 多 Controller、大规模部署 | P2 |

当前 Phase 1 的存储模型除了 Job / Event，还包含：

- 最新 Flow 别名（按 `flow_id` 查询）
- Flow 版本历史（按 `(flow_id, version)` 查询）
- 原始 WASM 字节（用于重启后重建模块缓存）

## Context 传递

- 小 context（< 256KB，可配置阈值）：直接内联在消息中
- 大数据：由 WASM action 通过 network/storage 能力自行处理，框架只传引用
- 不强制引入共享存储（S3/NFS），减少运维负担
