# 运维与观测

## 节点管理

以下内容仍属于分布式阶段目标：

- Node 注册 / 发现
- 心跳与健康检查
- 优雅停机 / drain

## 观测性

当前 Phase 1 已实现：

- `tracing` 日志接入
- JSON tracing 同时输出到终端，并按天滚动落盘到 `data-dir/logs/shirohad.log.YYYY-MM-DD`
- Job 生命周期、状态转移和 action 完成事件

以下仍属于后续阶段目标：

- OpenTelemetry / Prometheus 指标
- 分布式追踪
- 节点上下线事件

## 拓扑模式

### 模式一：有 Controller（中心化协调）

```
Controller ──── gRPC ────► Node A
    │                      Node B
    │                      Node C
    └── 负责：Job 调度、状态管理、Execution 分发、结果聚合
```

适合企业内部编排，有明确调度需求、需要全局视图和监控。

### 模式二：无 Controller（去中心化 P2P）— 远期目标

```
Node A ◄──── P2P ────► Node B
  │                      │
  └──────── P2P ────────►Node C
```

适合边缘计算、IoT 场景。需要额外解决 Leader 选举、状态同步、Execution 竞争认领等问题。

两种模式共享 Dispatch / Execution / Storage 层，区别仅在 Orchestration 层。
