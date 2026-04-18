# Workspace 与 Crate 划分

## 目录结构

```
shiroha/
├─ Cargo.toml                  # workspace
├─ wit/                        # 共享 WIT 世界定义 (package shiroha:host)
├─ proto/                      # 共享 .proto
├─ crates/
│  ├─ shiroha-core/            # 纯状态机: State/Event/Transition/Guard, 无 IO 无异步
│  ├─ shiroha-wit/             # wit-bindgen 生成的 host/guest 绑定
│  ├─ shiroha-wasm/            # Wasmtime runtime + WIT host impls + wasm 缓存(按哈希)
│  ├─ shiroha-dispatch/        # 分发策略: Local/Single/Broadcast/Shard/Weighted
│  ├─ shiroha-aggregate/       # 聚合策略: First/All/Quorum/Reduce/Custom
│  ├─ shiroha-transport/       # Transport trait + 注册/发现抽象
│  ├─ shiroha-transport-grpc/  # gRPC 实现 (tonic)
│  ├─ shiroha-proto/           # tonic-prost-build 产物
│  ├─ shiroha-storage/         # redb: FSM 定义/实例/事件日志/wasm blob
│  ├─ shiroha-controller/      # 控制端库: 实例机、调度器、聚合协调器
│  ├─ shiroha-node/            # 节点端库: 注册、心跳、执行 action (对应二进制 shiroha-agent)
│  └─ shiroha-config/          # config 加载 + 校验
└─ apps/
   ├─ shirohad/                # 主控二进制 (可内嵌本地 agent)
   ├─ shiroha-agent/           # 节点二进制
   └─ sctl/                    # 用户 CLI, 调 OrchestratorSvc
```

## Crate 职责

| Crate | 职责 |
|---|---|
| `shiroha-core` | 状态机数据结构、转换语义、错误类型；无 IO、无异步 |
| `shiroha-wit` | WIT 接口与 bindgen 产物 |
| `shiroha-wasm` | Wasmtime Engine 封装；加载 FSM 定义；执行 Action；host 侧 WIT 实现 (kv/http/clock/log 等) |
| `shiroha-dispatch` | 分发策略 trait 与内置实现；解析 FSM 定义中的声明 |
| `shiroha-aggregate` | 聚合策略 trait 与内置实现；自定义 reduce 调 wasm |
| `shiroha-transport` | Transport trait、节点标识、注册/发现抽象 |
| `shiroha-transport-grpc` | tonic 实现 |
| `shiroha-proto` | tonic-prost-build 生成的 gRPC stub |
| `shiroha-storage` | redb 持久化: FSM 定义、实例、事件日志、wasm blob |
| `shiroha-controller` | FSM 实例管理、调度协调器、聚合入口、承载 gRPC 服务 (对应二进制 `shirohad`) |
| `shiroha-node` | 节点注册、心跳、接受 Action 请求并转发至 wasm (对应二进制 `shiroha-agent`) |
| `shiroha-config` | 配置加载与校验 |

## 依赖方向

- `shiroha-core`、`shiroha-wit`、`shiroha-proto`、`shiroha-config` 为叶子依赖，不依赖其它业务 crate。
- `shiroha-wasm` 依赖 `core` 与 `wit`。
- `shiroha-dispatch`、`shiroha-aggregate`、`shiroha-transport`、`shiroha-storage` 依赖 `core`；`aggregate` 需要 `wasm` 以支持自定义 reduce。
- `shiroha-transport-grpc` 依赖 `transport` 与 `proto`。
- `shiroha-controller` 汇总上游全部依赖；`shiroha-node` 依赖 `wasm` 与 `transport`。
- 二进制 crate (`apps/*`) 仅依赖对应库 crate，不承担业务逻辑。

依赖图保持单向，禁止环引用。
