# 可插拔后端（Native Trait）

## Transport

节点间通信，通过 trait 抽象。核心能力：向指定 Node 发送消息、广播、订阅主题。

| 后端 | 场景 | 实现优先级 |
| ------ | ------ | ----------- |
| In-Process (tokio mpsc) | standalone 模式、开发测试 | P0 |
| gRPC (tonic) | 生产集群、Controller↔Node | P0 |
| libp2p / QUIC | 去中心化 P2P、边缘计算 | P2 |
| NATS | 事件驱动、异步解耦 | P2 |

## Storage

状态持久化，通过 trait 抽象。

| 后端 | 场景 | 实现优先级 |
| ------ | ------ | ----------- |
| Memory | 开发测试 | P0 |
| Redb | 嵌入式生产环境 | P0 |
| SQLite | 单机生产、工具链友好 | P1 |
| PostgreSQL | 多 Controller、大规模部署 | P2 |

## Context 传递

- 小 context（< 256KB，可配置阈值）：直接内联在消息中
- 大数据：由 WASM action 通过 network/storage 能力自行处理，框架只传引用
- 不强制引入共享存储（S3/NFS），减少运维负担
