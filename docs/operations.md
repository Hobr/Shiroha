# 运维与观测

## 节点管理

- **注册/发现**：Node 启动时向 Controller 注册，上报能力标签
- **心跳**：Node 定期上报健康状态和负载信息
- **健康检查**：Controller 检测 Node 存活，超时标记为不可用
- **优雅停机**：Node 下线前通知 Controller，drain 当前任务

## 观测性

统一到 tracing + OpenTelemetry 栈：

- **结构化日志**：tracing crate（兼容 log facade）
- **分布式追踪**：trace-id 贯穿 Controller → Node → WASM 执行
- **Metrics**：Prometheus / OpenTelemetry 导出
- **事件系统**：状态转移、任务完成、节点上下线等关键事件

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
