# Shiroha Version Roadmap

> 长期版本规划 — 以分布式调度为核心，Plugin 架构推迟到成熟阶段

---

## 设计原则

1. **分布式优先** — 框架核心价值在于 HSM + do-activity 分发，优先验证架构可行性
2. **可执行先行** — 早期就有可运行的二进制，便于演示和迭代验证
3. **Plugin 是锦上添花** — 扩展能力重要但非核心路径，推迟到 v0.7.0+
4. **每版本一个验证点** — 避免大而全，聚焦单一目标快速迭代

---

## ✅ v0.2.x — 基础验证（已完成）

### v0.2.0
- WIT 接口定义
- WASM adapter 结构（占位）
- IR 类型系统
- Engine 运行时核心

### v0.2.5
- **完整 WASM action 执行**（非占位）
- WasmActionInvoker 真实实现
- WasmAdapter 从 component 加载 IR
- Host import (`host.log`)
- 集成测试验证端到端流程

**验证点**: WASM Component Model 集成可行，IR → Task → Action 链路正确

---

## 🎯 v0.3.x — 单机可执行

### v0.3.0 — 基础守护进程
**shirohad**:
- 加载单个 WASM component
- TaskManager 本地管理 task
- 基础 CLI 参数（`--component <path>`）
- Tracing 日志输出
- 守护进程模式（后台运行）

**sctl**:
- 空壳占位
- 仅 `--help` 和版本输出
- 预留命令结构（不实现）

**验证点**: shirohad 能作为独立进程加载 WASM component 并运行 task

**不做**:
- ❌ REPL 交互
- ❌ gRPC
- ❌ 多 component 加载

---

### v0.3.5 — 本地交互增强
**shirohad**:
- 简单 REPL：手动输入事件触发迁移
- 输出当前状态 + action 日志
- 支持加载多个 component（手动指定多个 `--component`）
- 基础命令：`status` / `list-tasks` / `send-event`

**sctl**:
- 本地 Unix socket 连接 shirohad
- 实现基础命令：`list-tasks` / `send-event` / `task-status`
- 简单文本协议（JSON over Unix socket）
- 无认证

**验证点**: 本地控制工具可操作运行中的 shirohad，交互式验证状态机行为

**不做**:
- ❌ gRPC（推迟到 v0.4.0）
- ❌ 远程连接

---

## 🔧 v0.4.x — 控制面协议

### v0.4.0 — gRPC 协议定义
**gRPC service 定义**（proto 文件）:
- `ShirohaControl` — sctl ↔ shirohad 控制面
  - `CreateTask(component_path, task_id) -> TaskHandle`
  - `ListTasks() -> list<TaskInfo>`
  - `SendEvent(task_id, event_name) -> ()`
  - `GetTaskState(task_id) -> StateInfo`
  - `StopTask(task_id) -> ()`
- `NodeExecutor` — controller ↔ node 执行面（**仅定义，不实现**）
  - `ExecuteDoActivity(task_id, activity_name, payload) -> Result`
  - `CancelActivity(activity_id) -> ()`
  - `ReportResult(activity_id, result) -> ()`

**shirohad**:
- 启动 gRPC server（实现 `ShirohaControl`）
- 移除 Unix socket，全部走 gRPC
- 支持 `--listen <addr:port>` 参数

**sctl**:
- gRPC client 连接 shirohad（`--endpoint <addr:port>`）
- 实现所有 `ShirohaControl` 命令
- 支持远程连接

**验证点**: 控制面协议定型，sctl 可远程操作 shirohad

**不做**:
- ❌ 分布式（NodeExecutor 仅定义）
- ❌ 认证/授权（占位）
- ❌ TLS

---

## 🌐 v0.5.x — 分布式架构（核心价值）

### v0.5.0 — 分布式基础
**架构分离**:
- **Controller 角色**（shirohad `--mode controller`）:
  - 接受 sctl 的 task 创建请求
  - 管理全局 task 状态（纯内存）
  - **不执行 do-activity**，仅调度到 node
  - 维护 node 注册表（静态配置，`--nodes <addr1,addr2>`）
- **Node 角色**（shirohad `--mode node --controller <addr>`）:
  - 连接到 controller（gRPC client）
  - 注册自己为可用 node
  - 接收 do-activity 任务并执行
  - 返回执行结果到 controller
- **Local 角色**（shirohad `--mode local`，默认）:
  - Controller + Node 合体（v0.3.x 兼容模式）
  - do-activity 本地执行

**通信**:
- 实现 `NodeExecutor` gRPC service
- Controller 推送 do-activity 到 node（RPC 调用）
- Node 上报结果到 controller（response）

**do-activity 定义**:
- 状态机定义中标记 action 为 do-activity（`do: true`）
- Entry/exit action 仍本地执行（同步副作用）
- do-activity 可 async、可取消、可分发

**验证点**: Controller + Node 分离部署，do-activity 可跨节点执行

**不做**:
- ❌ 负载均衡（node 池只是列表，顺序调用）
- ❌ 容错（node 挂了 activity 失败，不重试）
- ❌ 持久化
- ❌ Node 健康检查

---

### v0.5.5 — 分布式增强
**Controller**:
- Node 健康检查 + 心跳机制
  - Node 定期发送心跳（`Heartbeat() -> ()`）
  - 超时未响应标记为 unavailable
- do-activity 失败重试（简单重试，无幂等性保证）
  - 最多重试 3 次
  - 重试间隔指数退避
- Round-robin 负载均衡
  - 从可用 node 池轮询选择
  - 不考虑 node 负载

**Node**:
- 并发执行多个 do-activity
  - 每个 activity 独立 tokio task
  - 资源限制（最大并发数配置 `--max-concurrent <n>`）
- 执行队列
  - 超出并发限制时排队等待
  - 队列满拒绝新任务

**验证点**: 分布式调度可用，基础容错和负载均衡

**不做**:
- ❌ 持久化
- ❌ 幂等性保证
- ❌ 智能调度（亲和性/优先级）

---

## 📊 v0.6.x — 分布式可靠性

### v0.6.0 — 持久化与恢复
**Controller**:
- Task 状态持久化
  - SQLite（单机部署）或 PostgreSQL（生产部署）
  - 表结构：tasks / do_activities / activity_results
- Controller 重启后恢复 task 状态
  - 加载未完成的 task
  - 重新调度未完成的 do-activity
- do-activity 幂等性标记
  - 状态机定义中标记 `idempotent: true`
  - 幂等 activity 可安全重试

**Node**:
- Graceful shutdown
  - SIGTERM 信号处理
  - 完成当前 activity 后退出
  - 通知 controller 注销
- 本地执行日志
  - Activity 执行历史（最近 N 条）
  - 用于调试和审计

**验证点**: Controller 重启不丢失 task 状态，Node 优雅退出

**不做**:
- ❌ 分布式事务
- ❌ 多 controller 高可用

---

### v0.6.5 — 安全与治理
**传输层认证**:
- mTLS 支持（双向证书认证）
  - Controller / Node / sctl 均需证书
  - 证书配置 `--tls-cert` / `--tls-key` / `--tls-ca`
- JWT token 认证（简化部署场景）
  - sctl 携带 JWT token 连接 controller
  - Controller 验证 token 签名和过期时间

**Capability 授权**（第一次实现）:
- Task 创建时声明 capability 需求
  - 从 WASM component metadata 读取
  - 例：`["network.http", "fs.read:/data"]`
- Controller 检查授权策略
  - 简单 allowlist（TOML 配置）
  - 拒绝超出授权的 task 创建
- Node 执行时二次校验
  - Action 执行前检查 capability
  - WASM 沙箱隔离（WASI capability）

**Audit log**:
- 所有控制面操作记录
  - CreateTask / StopTask / SendEvent
  - 包含：timestamp / caller / task_id / result
- 输出到 tracing 或独立日志文件

**验证点**: 安全生产部署就绪，满足基本治理要求

**不做**:
- ❌ 细粒度 RBAC
- ❌ 动态策略更新

---

## 🔌 v0.7.x — Plugin 架构（终于轮到）

### v0.7.0 — 扩展点定义
**Plugin 系统**:
- `PluginRegistry` + `Plugin` trait
  - immutable-after-init 模式（Arc without locks）
  - 按 plugin type name 索引
- 五个能力面 trait 定义（**全部 stub**）:
  1. `ActionFunc` — action 实现源（http/bash/...）
  2. `Middleware` — 横切关注点（日志/监控/认证）
  3. `Transport` — 分布式通信协议（gRPC/NATS/libp2p）
  4. `AggregationStrategy` — 结果聚合策略（第二层）
  5. `Adapter` — 状态机定义来源（扩展 IR adapter）

**ActionRef 两层语义**:
- `plugin: String` — plugin type name（如 "http"）
- `name: String` — action instance name（如 "fetch-user-api"）
- 第一层：plugin type → ActionFunc lookup
- 第二层：action instance name → payload config

**CompositeActionInvoker 路由**:
- 修改为传递 `ActionRef`（当前传递 `&str`）
- Wasm / Plugin 分支路由
- Plugin 通过 registry 查找

**验证点**: Plugin 架构就位，可注册和路由 stub plugin

**不做**:
- ❌ 具体 plugin 实现（推迟到 v0.7.5+）
- ❌ Plugin 配置系统

---

### v0.7.5+ — 具体插件实现
**HTTP ActionFunc**:
- 基于 reqwest 实现
- 支持 GET/POST/PUT/DELETE
- 配置从 payload 传递（JSON）
- 超时 / 重试 / headers 可配置

**Bash ActionFunc**:
- 基于 tokio::process::Command
- 沙箱隔离（禁止访问敏感路径）
- 环境变量注入
- stdout/stderr 捕获

**NATS Transport**（可选，替代 gRPC）:
- Controller ↔ Node 通信走 NATS
- Pub/Sub 模式（controller 发布任务，node 订阅）
- 解耦 controller 与 node

**Plugin 配置系统**:
- 从 TOML 加载 plugin 列表
  ```toml
  [plugins.http]
  type = "http"
  timeout = "30s"
  max_redirects = 5
  ```
- 运行时解析并注册到 registry

**验证点**: 第一批真实 plugin 可用，HTTP/Bash action 经过生产验证

---

## 🎨 v0.8.x+ — 高级特性（未来）

### 可能方向
- **智能调度**: Node 亲和性 / 任务优先级 / 资源预留
- **多 Controller 高可用**: Raft 共识 / 主备切换
- **WebAssembly plugin**: Plugin 本身编译为 WASM（沙箱隔离）
- **可观测性增强**: OpenTelemetry 集成 / Metrics / 分布式追踪
- **GUI/Web 控制台**: 可视化 task 管理
- **状态机可视化**: Graphviz 渲染 / 实时状态展示
- **文件 adapter**: JSON/TOML/YAML 状态机定义

---

## 📅 里程碑总结

| 版本 | 核心目标 | 验证点 | 预计复杂度 |
|------|---------|--------|----------|
| v0.3.0 | 单机跑起来 | shirohad 能加载 WASM 并运行 task | 低 |
| v0.3.5 | 本地交互 | sctl 能控制本地 shirohad | 低 |
| v0.4.0 | 协议定型 | gRPC service 定义完整 | 中 |
| v0.5.0 | 分布式基础 | controller + node 可分离部署 | **高** |
| v0.5.5 | 分布式可用 | node 池 + 简单负载均衡 | 中 |
| v0.6.0 | 分布式可靠 | 持久化 + 重启恢复 | 高 |
| v0.6.5 | 生产就绪 | 认证 + 授权 + 审计 | 中 |
| v0.7.0 | 扩展能力 | Plugin 架构 | 中 |
| v0.7.5+ | 插件生态 | HTTP/Bash/NATS plugin | 低 |

---

## 关键决策记录

### 为什么分布式优先于 Plugin？
- **核心价值**: Shiroha 的差异化能力在于 HSM + do-activity 分发，不是 action 扩展
- **架构验证**: 分布式调度的可行性更重要，plugin 是成熟阶段的优化
- **依赖关系**: Transport/AggregationStrategy trait 在分布式实现后再抽象更合理

### 为什么可执行文件先于分布式？
- **迭代验证**: 单机 shirohad 是分布式的基础（node 本质是 shirohad 的子集）
- **早期演示**: 有可运行的二进制便于展示和用户反馈
- **降低风险**: 先验证 WASM 加载 + task 管理流程，再扩展到分布式

### 为什么 Plugin 推迟到 v0.7.0？
- **非核心路径**: HTTP/Bash action 可以硬编码实现，不影响框架核心能力
- **过度设计风险**: 过早抽象 plugin 系统可能导致接口不稳定
- **优先级**: 分布式调度 + 可靠性 + 安全治理更重要

---

## 下一步

当前位置：**v0.2.5 完成**  
下一个目标：**v0.3.0 单机可执行**

创建任务：`python3 ./.trellis/scripts/task.py create "v0.3.0: shirohad 单机守护进程" --slug v0.3.0-shirohad`
