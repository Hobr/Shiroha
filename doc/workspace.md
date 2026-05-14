# Workspace 布局

## 目标

- 通过 crate 边界强制实现「职责正交」
- 单向依赖,无环;新增传输或存储后端只需新增 crate,不动现有 crate
- 二进制 (`apps/`) 只做装配,不写业务逻辑

## Crate 列表

`crates/` 下放置库 crate。每个 crate 至多承担一个职责。

| Crate | 职责 | 允许的依赖方向 |
|---|---|---|
| `shiroha-core` | FSM/Action(含 WaitingMode)/ComponentId/Job/分发策略/聚合策略的纯 domain 类型与 trait;零 I/O。**Flow 不在 core**,见 storage.md | 仅标准库 + 序列化 |
| `shiroha-wit` | `.wit` 接口文件与 wit-bindgen 生成绑定 | core |
| `shiroha-wasm` | Wasmtime 集成、组件加载、Host 能力实现、Action 调用桥 | core, wit |
| `shiroha-dispatch` | Dispatcher + Aggregator;在 Executor 之上做位置选择与聚合 | core, wasm, transport |
| `shiroha-transport` | 节点间 RPC 抽象 trait | core |
| `shiroha-transport-grpc` | tonic 实现的 transport | transport, proto |
| `shiroha-proto` | tonic 生成的代码(节点面 + 控制面);独立 build.rs | — |
| `shiroha-storage` | Store trait + redb 默认实现;**Flow 版本管理与 Component 去重存储是主控层独有职责,放在本 crate** | core |
| `shiroha-engine` | 主控:Job 调度、状态驱动、事件日志 | core, wasm, dispatch, storage |
| `shiroha-worker` | 节点:接收 Action,调用本地 Executor,回报结果 | core, wasm, transport |
| `shiroha-control` | 控制面 gRPC 服务定义与实现 | core, engine, proto |
| `shiroha-config` | 统一配置加载 | — |

`apps/` 下放置二进制:

| App | 角色 |
|---|---|
| `shirohad` | 装配 engine + worker + control + transport,按配置选择运行模式 |
| `sctl` | 装配控制面客户端 + clap CLI |

## 依赖方向规则

```
                ┌───────────────┐
                │  shiroha-core │   ← 所有 crate 的根
                └───────┬───────┘
        ┌───────────────┼───────────────┐
        ▼               ▼               ▼
   shiroha-wit    shiroha-storage   shiroha-transport
        │                                │
        ▼                                ▼
   shiroha-wasm                  shiroha-transport-grpc
        │
        ▼
  shiroha-dispatch ◀────────────── shiroha-worker
        │
        ▼
   shiroha-engine ◀── shiroha-control ◀── shiroha-proto
        │
        ▼
    apps/shirohad                  apps/sctl
```

硬性约束:

- `core` 不依赖任何其它 shiroha crate
- `wasm` 不依赖 `transport`,反之亦然
- `transport` 不依赖 `storage`,反之亦然
- `engine` 是装配点,可以同时依赖多个底层 crate
- `apps/` 只做装配:把 trait 实现注入到对应抽象点
- 不允许出现下游 crate 反向引用上游 crate

任何打破上述方向的依赖应当先在 PR 中讨论。

## 命名约定

- crate 名一律以 `shiroha-<area>` 为前缀
- 后端实现使用 `shiroha-<area>-<backend>` 形式 (例:`shiroha-transport-grpc`、未来的 `shiroha-transport-quic`)
- 二进制名沿用项目惯例:控制端 `sctl`,守护进程 `shirohad`
- 测试辅助 crate 后缀 `-testkit`,只用于 `[dev-dependencies]`

## 演化策略

- 新增传输协议:增加 `shiroha-transport-<name>`,实现 transport trait,主二进制按 feature 启用
- 新增存储后端:增加 `shiroha-storage-<name>` 或在 `shiroha-storage` 内通过 feature 切换
- 新增控制面客户端 (TUI/Web):新建二进制 crate,复用 `shiroha-proto` 的客户端 stub
- 拆分 crate 而不是塞功能:任何超过 ~1000 LOC 的 crate 应考虑是否还能再切
