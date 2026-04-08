# 路线图

## Flow 验证

部署 Flow 时进行静态检查：

- 不可达状态检测
- 死锁检测（环路无出口）
- WASM 函数引用验证（通过 guest 导出的 `supports-action` / `supports-guard` / `supports-aggregate` 接口校验 manifest 中命名的引用）
- 权限匹配验证（模块使用的 import 是否匹配声明的 world）

当前审计状态：

- 已实现：不可达状态检测、死锁/无出口检测、manifest `host-world` 与 component imports 匹配校验，以及 deploy 期对 `action` / `guard` / `aggregator` 名称的 guest 支持校验

## 分阶段实施

### Phase 1 — 单机可用（MVP）

目标：在单进程内跑通完整链路。

- shirohad standalone 模式（Controller + Node 同进程，in-process 通道）
- 状态机核心引擎：State / Transition / Event 驱动
- Job 并发控制：每个 Job 串行化事件处理；paused 状态下事件持久化排队
- 定时器：transition timeout，Controller 本地 timer wheel
- wasmtime 集成：加载 WASM、调用 get-manifest / invoke-action / invoke-guard
- 基础 WIT host 接口（sandbox world）
- 状态级 hook：`on-enter` / `on-exit`
- Job 生命周期：running / paused / cancelled / completed
- Job `max_lifetime`：超时自动取消
- 事件溯源：状态转移事件写入 Storage（与状态更新同事务）
- Flow 版本绑定：旧 Job 继续使用创建时绑定的版本
- Redb 持久化（最新 Flow 别名 + 版本历史 + 原始 WASM 字节）
- 重启恢复：重新加载 Flow 版本和模块缓存，恢复 Job 快照、暂停事件队列和 timeout 计划
- sctl CLI：部署/列出/查看 Flow，创建/列出/查看/等待 Job，触发事件、暂停/恢复/取消 Job，查询事件日志
- tracing 结构化日志

当前状态：

- Phase 1 的 standalone 主链路已闭合：`remote` 已通过 in-process transport 进入同进程 node worker，`fan-out` 已支持结果收集、`aggregate()` 调用与 follow-up event 推进，tracing 已同时输出结构化日志到终端和滚动文件
- 仍未进入 Phase 1 范围之外的能力，继续按 Phase 2+ 路线推进：独立进程 Node、真实多节点集群调度、自动 `subprocess` 编排、OpenTelemetry、mTLS 等

### Phase 2 — 分布式

目标：Controller 和 Node 可以分开部署。

- Controller / Node 进程分离
- gRPC transport（tonic）：任务下发、结果回报、心跳
- 节点认证：Join Token 方案
- Node 注册 / 发现 / 健康检查
- WASM 模块分发 + Node 端缓存
- 基础调度（round-robin）
- 任务超时 + 重试
- fan-out 分发 + 聚合
- 通用持久化 event inbox

### Phase 3 — 生产就绪

目标：可以在生产环境运行。

- OpenTelemetry metrics + 分布式追踪
- 优雅停机 / Node drain
- 节点认证升级：mTLS
- 配置热加载
- WASM 权限系统（network / storage / full world）
- 高级调度（负载感知、能力标签）
- WASM Plugin 体系（scheduler / middleware 插件槽）
- in-flight Action 跟踪 / 取消 / 恢复
- 子流程：subprocess 状态类型、父子 Job 关联
- Web 管理界面
- Flow 版本生命周期管理（清理策略、保留策略、历史查询）
- 事件溯源回放 API

### Phase 4 — 生态扩展

- 更多 transport 后端（QUIC / NATS）
- 更多 storage 后端（SQLite / PostgreSQL）
- WASM 模块热更新
- Flow 灰度发布
- 多 Controller 高可用（Raft）
- 去中心化 P2P 模式
