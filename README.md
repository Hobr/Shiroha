# Shiroha

> 由WebAssembly驱动的分布式状态机任务编排框架

## 核心概念

### 架构层级

- **Controller** (可选的协调点)
  - 负责全局 Flow 管理
  - 负责 Job 调度和生命周期
  - 可选：可以有多个 Controller/无 Controller
  - 用于中央协调/监控和管理

- **Executor** (分布式执行组件)
  - 负责 Execution 的实际执行
  - 可以执行 Guard/Action/Work
  - 无状态或轻量级状态
  - 支持多种执行模式(WASM/容器/HTTP等)

### 流程和执行

- **Flow** (静态状态机定义)
  - 由 WASM 模块初始化时定义
  - 包含：States/Transitions/DispatchPolicies
  - 不可在运行时修改(除了 DynamicState)
  - 可验证/可视化/可缓存

- **Job** (Flow 的执行实例)
  - 代表一个具体的工作流执行
  - 有当前状态/执行历史/上下文数据
  - 可以暂停/恢复/重试
  - 支持子 Job(SubFlow)

- **Execution** (最小执行单元)
  - 可以是：Guard 评估/Action 执行/Work 执行
  - 有分发策略(DispatchPolicy)
  - 可以本地执行或分布式执行
  - 可以复制(Replicated)/分片(Sharded)/或自定义

### 分布式编排

- **Guard** (状态转移条件)
  - 在转移前执行，决定是否允许转移
  - 返回 bool
  - 可以有 DispatchPolicy(通常 singleton 或 replicated)
  - 由 WASM 提供

- **Action** (转移时的副作用)
  - 在转移时自动执行
  - 可以修改 context
  - 可以有 DispatchPolicy(通常 singleton)
  - 用于日志/统计/通知等

- **Work** (实际业务操作)
  - Flow 中的节点(状态)
  - 由 Executor 执行
  - 支持多种执行模式(WASM/容器/HTTP)
  - 必须有 DispatchPolicy(决定如何分布式执行)

### 容器和组合

- **Loop** (循环容器)
  - 在单个状态内迭代
  - 迭代次数由 Guard 动态决定
  - 轻量级/高效

- **Parallel** (并行容器)
  - 在单个状态内并行分支
  - 分支数由 Guard 动态决��
  - 自动聚合结果

- **SubFlow** (子流程)
  - 在状态中调用另一个 Flow
  - 创建独立的子 Job
  - 支持参数映射和结果聚合
  - 支持递归

- **DynamicState** (完全动态状态)
  - 图灵完整
  - WASM 驱动的动态状态创建
  - 用于极端场景

### 分布式策略

可应用于 Guard/Action/Work

- 模式：
  - **Singleton**:  1 个 Executor 执行
  - **Replicated**: N 个 Executor 投票决定(容错)
  - **Sharded**: M 个 Executor 分片处理(扩展)
  - **Custom**:  WASM 决定(灵活)

### 执行模式

- **ExecutionMode** (Work 的执行方式)
  - WASM:  执行 WASM 代码
  - Container: 运行容器(Docker/K8s)
  - HTTP: 调用外部 HTTP 服务
  - 其他：可扩展(基于原生代码/WASM 插件)

## 框架

- apps
  - [ ] shirohad 单一服务应用
    - [ ] controller 控制端
    - [ ] executor 执行端

  - [ ] sctl 命令行工具
  - [ ] shiroha-web Web界面
  - [ ] shiroha-desktop 桌面客户端
  - [ ] shiroha-mobile 移动客户端

- crates
  - [ ] shiroha-ir 中间表示, 所有数据结构的定义
    - [ ] Flow
    - [ ] Job
    - [ ] Execution
    - [ ] Guard
    - [ ] State
    - [ ] ExecutionMode
    - [ ] DispatchPolicy

  - [ ] shiroha-orchestrator 编排层, 负责 Flow 和 Job 的管理和执行
    - [ ] scheduler 调度器
    - [ ] dispatcher 分发器
    - [ ] FlowExecutor Flow 执行器
    - [ ] JobManager Job 管理器
    - [ ] DistributedExecution 分布式执行

  - [ ] shiroha-engine 执行层, 处理 Guard/Action/Work 的具体执行
    - [ ] guard 守卫执行
    - [ ] action 动作执行
    - [ ] work 工作执行
    - [ ] execution-handler 执行处理器

  - [ ] shiroha-runtime 运行时, 执行环境的运行时支持
    - [ ] wasm WASM
    - [ ] container 容器
    - [ ] HTTP 网络

  - [ ] shiroha-storage 存储, 数据持久化
  - [ ] shiroha-network 网络, 分布式通信
  - [ ] shiroha-error 错误处理, 统一的错误定义
  - [ ] shiroha-logger 日志
  - [ ] shiroha-metrics 指标
  - [ ] shiroha-tracing 追踪
  - [ ] shiroha-auth 认证

- plugins
  - [ ] wit WIT接口
  - [ ] shiroha-sdk-rs RustSDK
  - [ ] example 示例
  - preset 预置

## 开发

```bash
git clone https://github.com/Hobr/Shiroha.git
cd Shiroha

# 环境
apt install rustup just cargo-binstall

# 构建
just build

# 开发
pip install pre-commit
just install-dev
just fmt
just doc

# 更新
just update

# 发布
just release
```
