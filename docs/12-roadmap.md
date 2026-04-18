# 短期实现与预留

## 范围对照

| 关注点 | 现在 | 预留口子 |
|---|---|---|
| 传输 | `shiroha-transport-grpc` | trait 已定义, 追加 `-quic` / `-mq` 即可 |
| 前端 | `sctl` | Orchestrator gRPC 同时可供 TUI / Web |
| 聚合 | First / AllOk / Quorum | `Reduce` 走现成 WasmRuntime |
| 调度 | Local + Single + Broadcast | 后加 Shard / Weighted，不改 Controller |
| 节点发现 | 静态配置 + 注册 gRPC | trait 化后可接 etcd / consul |
| 持久化 | redb 单机 | 抽象存储接口，后续可替换 |

## 里程碑

- **M0 Skeleton**：workspace + `core` + `wit` + `wasm` (单机执行) + `sctl` 跑通单实例单 Action。
- **M1 Persistence & Timers**：接入 `storage`；主控重启后能恢复实例与事件日志；实现持久化的**超时调度器**；`StreamEvents` 支持游标续传。
- **M2 Distributed**：`proto` + `transport-grpc` + `shiroha-agent` 二进制；`dispatch` 支持 Local / Single / Broadcast；`aggregate` 支持 First / AllOk / Quorum；引入 `attempt_id` 与调用 / 心跳双档超时。
- **M3 Custom Aggregate & Health**：WASM 自定义 reduce；节点心跳、健康检查、故障剔除；可观测性贯穿 (trace / metrics)。
- **M4 Expansion**：Quic / MQ transport；TUI / Web 前端；节点发现接入外部注册中心；mTLS 与身份鉴别。

## 远期规划

### 用户自定义分发 / 聚合策略 (WASM)

当前 `shiroha-dispatch` 与 `shiroha-aggregate` 的策略由框架内置枚举提供，用户只能在 FSM 定义中选择；自定义聚合的 `Reduce` 是唯一的 WASM 扩展点。

远期目标：让用户通过 WASM 自行实现**分发策略**与**聚合策略**，以元数据形式在 FSM 定义中引用对应的 guest 导出函数。

- **分发侧**：guest 导出一个函数，输入为 Action 元数据与当前可用节点视图，输出为 `DispatchPlan`。主控在独立沙箱中调用，不直接参与调用编排。
- **聚合侧**：扩展现有 `Reduce` 机制，允许完整替换 `Aggregator`，支持流式消费与中途短路。
- **约束**：自定义策略函数必须为纯函数 (无 host IO 或仅限只读能力)，执行时间受沙箱超时约束，失败时回落到内置策略或整体失败 (由定义声明)。
- **接口冻结**：内置枚举与 WASM 策略共用同一份 `DispatchPlan` / 聚合协议；新增能力通过 WIT 扩展，不破坏现有 FSM 定义。

此项属于 M4 之后的议题，落地前需先稳定 `shiroha-dispatch` / `shiroha-aggregate` 的 trait 契约与 WIT 对应接口草案。

## 风险与暂缓项

- 多主控 / 高可用：暂不在路线内，留待 M4 之后视需求设计。
- WASM 热升级：当前以哈希即版本的方式规避；实例级迁移策略留作未来议题。
- 权限与多租户：WIT 能力已按最小权限切分，完整租户隔离留待独立专题。
- 审计加密：主控 redb 文件仅靠文件权限保护，应用层加密留作远期。
- 补偿 / Saga：聚合失败不自动补偿；用户需在状态机中显式建模错误路径。
