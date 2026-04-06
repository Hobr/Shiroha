# Shiroha Comprehensive Refactor Design

**Date:** 2026-04-06

## Goal

对当前仓库做一次面向初始阶段项目的全面重构，优先解决重复实现、职责混杂、边界不清和明显不合理的查询路径，同时保持对外协议与 CLI 兼容。

## Context

当前仓库整体可编译、可测试，但已经出现几类会持续放大维护成本的问题：

- `shirohad` 中 Flow 内存注册表和引擎缓存的维护逻辑在启动恢复、部署路径和测试辅助中重复实现。
- `job_service.rs` 同时承担 gRPC handler、参数校验、事件过滤、运行态编排、WASM 调用和 timeout 调度，单文件职责过重。
- `flow_service.rs` 同时承担组件校验、部署编排、注册表维护和查询响应装配，边界不稳定。
- 存储层部分查询语义没有沉到后端实现，service 仍在做全量读取后过滤。
- 测试中反复手写完整 `FlowManifest`、版本注册逻辑和轮询等待逻辑，测试自身已形成重复源。
- `sctl` 和 `shiroha-client` 的职责分离方向是对的，但内部辅助逻辑还在继续膨胀。

这些问题现在还可控，但如果继续叠加功能，会让后续开发持续在重复代码和隐式耦合上付出成本。

## Non-Goals

本次重构不做以下事情：

- 不修改 gRPC proto 对外字段和服务接口。
- 不修改 CLI 命令面和 README 中的主要用法。
- 不引入新的 workspace crate。
- 不改变 `FlowManifest` / WIT world 的外部语义。
- 不把未实现能力伪装成已完成能力，例如 standalone 下的真正 fan-out 调度。

## Design Principles

- 单一职责优先于文件数量最少。
- 把“语义”下沉到最合适的层，而不是让上层补逻辑。
- 优先消除会影响正确性的重复逻辑，其次再做结构整理。
- 先稳定测试支撑，再重排生产代码。
- 对外兼容优先，内部接口可以重塑。

## Target Architecture

### 1. `app/shirohad`

引入以下内部模块：

- `src/flow_registry.rs`
  - 统一管理 latest/versioned `FlowRegistration`
  - 统一管理 latest/versioned `StateMachineEngine`
  - 提供部署注册、启动恢复、删除和测试注册入口
  - 消除 `server.rs`、`flow_service.rs`、测试辅助中的重复 cache 更新逻辑

- `src/job_runtime.rs`
  - 封装 Job 运行时执行链
  - 负责 guard、transition、action、state hook、timeout 重建
  - 让 `job_service.rs` 不再承担完整执行编排

- `src/job_events.rs`
  - 封装事件查询参数校验
  - 封装 `since_id` / `since_timestamp_ms` / `kind` / `limit` 的筛选逻辑
  - 让事件读取语义独立于 gRPC handler

- `src/service_support.rs`
  - 统一 UUID 解析
  - 统一 `ShirohaError -> tonic::Status` 映射
  - 统一常见 invalid/not-found/precondition 构造

重构后的 `flow_service.rs` / `job_service.rs` 仅负责：

- 解包 gRPC 请求
- 调用领域辅助模块
- 组装 proto 响应

### 2. `crate/shiroha-store-redb`

保留 crate 边界不变，但把存储内部逻辑按语义拆清：

- Flow 相关读写 helper
  - flow latest/version key 生成
  - flow version 范围扫描
  - flow 删除时的版本级清理

- Job/Event 相关 helper
  - event 复合键生成
  - job 前缀事件范围扫描
  - delete job 时的 snapshot + event 清理

- Capability KV helper
  - namespace key 生成
  - 按 namespace/prefix 的 key 读取

这里允许保留单文件实现，但必须把重复的事务和扫描逻辑收敛成清晰 helper。若实现过程中单文件仍过长，可再拆成 `flow_store.rs`、`job_store.rs`、`kv_store.rs` 三块。

### 3. `crate/shiroha-core`

补充存储抽象的语义接口，使上层不再自己过滤全量结果。重点新增或收紧：

- 按 `flow_id` 列出版本历史
- 保持按 `job_id` 获取事件的明确语义
- 尽量让默认 `MemoryStorage` 与 `RedbStorage` 在语义上对齐

如无必要，不在 core 中引入额外复杂抽象。

### 4. `crate/shiroha-client` 和 `app/sctl`

继续保持“客户端抽象”和“CLI 展示层”分离，但进一步收紧内部职责：

- `shiroha-client`
  - 统一排序 helper
  - 统一绑定 Flow 查询 helper
  - 统一 event query 构造 helper

- `sctl`
  - `main.rs` 保留 clap 定义和 command dispatch
  - 输入字节解码、wait/follow 轮询、presenter 调用辅助从 `main.rs` 拆出

不追求把 CLI 拆得很碎，只保证 command parsing、业务编排、输出格式化三者边界清楚。

### 5. Tests

新增或收敛测试辅助，避免继续复制：

- 通用 manifest builder
  - approval flow
  - timeout flow
  - terminal target flow

- 通用 flow version register helper
  - 用于 service 测试和 server 恢复测试

- 通用 job wait/poll helper
  - 用于事件驱动和 timeout 相关测试

目标不是让所有测试都通过共享夹具，而是消除当前明显重复的那一批。

## Interface Compatibility

以下边界必须保持兼容：

- `shiroha_proto` 生成的请求/响应消息
- `FlowService` / `JobService` 的对外行为
- `sctl` 的现有命令和主要输出契约
- `README.md` 中的主要命令入口

以下边界允许重构：

- `ShirohaState` 的内部组织
- service 文件内部私有方法
- `Storage` trait 的内部扩展
- 测试模块结构和测试 helper 位置

## Refactor Phases

### Phase 1: Test Support and Shared Boundaries

目标：

- 提取重复测试辅助
- 为后续重构建立稳定测试支撑
- 在不改变行为的情况下引入共享 helper

完成标准：

- `flow_service.rs`、`job_service.rs`、`server.rs` 中重复 manifest/register/wait helper 明显减少
- 相关测试仍通过

### Phase 2: Flow Registry Consolidation

目标：

- 引入 `flow_registry.rs`
- 收敛部署、恢复、删除时的内存 registry/cache 更新逻辑
- service 和测试都通过统一入口操作 flow 注册信息

完成标准：

- 不再在多个文件中手工同步 `flows` / `flow_versions` / `engines` / `versioned_engines`
- 启动恢复和部署路径共用同一套注册语义

### Phase 3: Job Runtime and Event Query Extraction

目标：

- 引入 `job_runtime.rs` 和 `job_events.rs`
- 收敛事件处理执行链和事件过滤语义
- 简化 `job_service.rs`

完成标准：

- `job_service.rs` 不再承担完整运行态编排
- 事件筛选和参数校验拥有独立测试

### Phase 4: Storage Query Tightening

目标：

- 为 flow version 和 job event 读取引入更合理的后端查询路径
- 尽量把全量读取后过滤迁移到存储层

完成标准：

- `FlowService::list_flow_versions` 不再依赖全量读取后过滤
- `RedbStorage` 使用 key 语义做前缀/范围查询

### Phase 5: Client and CLI Internal Cleanup

目标：

- 收敛 `shiroha-client` 的重复辅助
- 收窄 `sctl` 的 `main.rs` 职责

完成标准：

- `main.rs` 主要保留命令定义和分发
- client 内部 helper 更聚焦，排序/绑定查询不散落

## Verification Strategy

每个阶段遵循 TDD：

1. 先迁移或新增测试，确认红灯或覆盖目标行为。
2. 做最小实现改动让测试回绿。
3. 再做局部重构，保持测试持续通过。

阶段性验证至少包括对应 crate 的定向测试。总体验证必须包括：

- `cargo check --workspace`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo nextest run --all-features --no-tests=warn`
- `just fmt`

## Risks and Controls

### Risk: 重构把行为变更和结构整理混在一起

控制：

- 对外行为不改
- 每次提交聚焦一个子域
- 每个子域先有测试再改实现

### Risk: 服务层拆分后出现状态同步遗漏

控制：

- Flow registry 统一入口
- 针对部署、恢复、删除、版本绑定写回归测试

### Risk: 存储优化引入查询顺序变化

控制：

- 明确排序语义由哪一层负责
- 让 `MemoryStorage` 和 `RedbStorage` 返回语义保持一致

### Risk: 测试重用过度导致可读性下降

控制：

- 只抽取重复度高的 builder/helper
- 保留各测试对关键业务意图的显式表达

## Expected Outcome

重构完成后，仓库应具备以下特征：

- service 文件显著收缩，职责边界清楚
- Flow registry/cache 维护逻辑不再重复
- 存储查询语义更贴近底层 key 设计
- 测试夹具减少重复且不牺牲可读性
- CLI/client 继续保持兼容，但内部结构更稳

这次重构的成功标准不是“文件变多或变少”，而是把重复逻辑和跨层耦合收敛到能持续演进的边界内。
