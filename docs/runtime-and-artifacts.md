# Shiroha Runtime And Artifacts

> Wasm 制品、WIT 接口、能力声明与运行时约束

## Status

- 状态：Draft
- 目标版本：v0.1 基线运行时，含部分更远期能力规划
- 当前实现：仓库尚未实现本文中的 Wasm 制品注册、WIT 校验、能力注入和调度匹配链路；本文是设计约束与路线说明
- 阅读约定：`必须` / `不能` 表示目标语义；`候选` / `建议` / `v0.4+` 表示未定或远期规划

## 核心概念

| 名词 | 说明 |
| --- | --- |
| `Artifact` | 可分发、缓存和校验的 Wasm 制品 |
| `ArtifactStore` | 保存原始 Wasm 字节码制品的存储层 |
| `CompiledModuleCache` | 保存节点本地 Wasmtime 编译结果的缓存层 |
| `WIT Interface` | 宿主暴露给 Wasm 的能力边界定义 |
| `Host Capability` | 由宿主提供给 Wasm 的具体功能 |
| `Permission Model` | 对 Wasm 和节点可用能力的限制与授权规则 |
| `Resource Label` | 节点声明的可调度属性, 如区域、架构、GPU、网络能力 |

## Workflow Definition

用户通过 Rust SDK 声明工作流, 编译为 Wasm 制品。声明方式在 v0.1 实现阶段确定，目前仍处于候选方案比较阶段：

- 宏驱动
- Trait 驱动
- 声明式 DSL

核心约束是: 最终产物必须是符合 WIT 契约的 Wasm 模块, 包含决策函数入口和 Activity 导出。

## Wasm 制品生命周期

### 版本绑定

- 新实例创建时绑定一个确定的 `Artifact`
- 已运行实例永远使用启动时绑定的 Wasm 版本
- 不处理运行中热切换 Wasm 的语义

### 制品注册

注册时至少校验:

- Wasm 制品可被当前运行时加载
- WIT / ABI 与 Host 兼容
- 制品声明的宿主能力在当前系统中存在
- 可选的签名、来源和发布元数据通过校验

建议的版本状态:

- `draft`
- `active`
- `deprecated`
- `disabled`

版本状态变化不能影响已运行实例已绑定的 Artifact。

### 制品保留与删除

即使 Artifact 版本状态为 `disabled`，如果仍有运行中的实例绑定该版本，其 Wasm 字节码不允许从 `ArtifactStore` 中物理删除。版本管理应通过引用计数或等价机制判断是否安全删除。

如果绑定版本的制品意外丢失（存储故障、误操作等），实例恢复将失败并进入 `Failed` 状态，直到管理员手动重新注册该版本或执行其他恢复操作。

## 编译缓存

- `ArtifactStore` 保存原始 Wasm 字节码
- `CompiledModuleCache` 保存节点本地的 Wasmtime 编译结果
- 编译缓存可失效重建, 不参与语义正确性判定

## WIT ABI 兼容策略

- Host 按 semver 管理 WIT 接口
- Wasm 制品声明绑定的 WIT package id 与兼容版本范围
- Host 显式声明支持的接口版本集合或兼容矩阵
- 注册时按 `package id + version` 校验兼容性, 不能只用数值大小比较
- v0.1 不承诺稳定兼容; v0.3+ 冻结接口后再严格执行语义版本管理

## Host 能力边界

Host 与 Wasm 的能力边界分为两层:

- `Decision` 侧: 只暴露确定性接口
- `Activity` 侧: 暴露经权限控制后的宿主能力

当前阶段能力规划:

- v0.1: `logging`、`workflow-time`、`random`
- v0.2-v0.3: 继续以最小接口集为主

注意能力分层:

- `workflow-time` (Decision 侧): 只读的流程计时，数据来源于已记录的事件时间戳或定时器触发，不是 wall-clock
- `random` (Activity 侧): 非确定性输入，仅允许在 Activity 中使用，不允许 Decision 直接访问
- `wall-clock` (Activity 侧): 实际系统时间，仅在 Activity 执行中可用，不允许 Decision 直接访问
- v0.4+: 扩展到 `http`、`file system`、`messaging`

## 能力声明、匹配与执行闭环

### 能力声明

- `Workflow Definition` / `Artifact` 应为每个 Activity 显式声明所需能力集合
- 注册阶段先校验这些能力名称和版本在当前系统中可识别

### 调度匹配

- 调度阶段根据 Activity 所需能力集合、`Resource Label`、节点健康状态和当前容量筛选可执行节点
- 不应将任务调度到不满足能力要求的节点

### 运行时授权

- 运行阶段由 `Permission Model` 对最终注入给 Wasm 的宿主能力做裁剪与授权
- 调度匹配与运行时授权必须形成闭环
- 不应出现“调度时可执行, 实际注入能力时才发现缺权限”的常态路径
- 如果由于配置漂移或版本变化导致运行时能力不满足, 应快速失败并记录明确错误

## 节点能力与调度关系

- 节点能力应显式声明而非隐式推断
- 能力声明可以包括网络访问能力、区域、CPU 架构、GPU、附加宿主能力集等
- 当多个节点都满足要求时, 调度器可采用轮询、最少在途任务或加权策略
- 当没有节点满足要求时, 任务应保持待调度状态, 不应静默降级执行
