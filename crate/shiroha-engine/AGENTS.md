<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-02 -->

# shiroha-engine

## Purpose

状态机引擎层。负责状态转移驱动、Job 生命周期管理、定时器轮、调度策略和 Flow 静态验证。不直接依赖 WASM 运行时。

## Key Files

| File | Description |
| ---- | ----------- |
| `src/engine.rs` | `StateMachineEngine`：根据当前状态+事件查找转移，无状态设计可被多 Job 共享 |
| `src/job.rs` | `JobManager<S: Storage>`：Job CRUD + 生命周期流转 + 事件溯源写入 |
| `src/timer.rs` | `TimerWheel`：基于 tokio::spawn 的定时器管理，支持按 Job 暂停/恢复 |
| `src/scheduler.rs` | `Scheduler` trait + `RoundRobinScheduler`（默认轮询调度） |
| `src/validator.rs` | `FlowValidator`：部署时静态检查（可达性、终态、函数引用） |

## For AI Agents

### Working In This Directory

- `StateMachineEngine` 是纯逻辑（给定状态+事件→转移结果），不管理 Job 状态
- `JobManager` 负责所有状态变更 + 事件溯源写入，泛型 `S: Storage` 允许注入不同后端
- `TimerWheel` 每个定时器是独立的 tokio::spawn 任务，暂停时 abort 并记录剩余时间
- 添加新调度策略：实现 `Scheduler` trait 即可

### Testing Requirements

- `cargo check -p shiroha-engine`
- FlowValidator 的检查逻辑适合写单元测试（纯函数，输入 manifest 输出 warnings）

### Common Patterns

- UUIDv7 用于所有 ID 生成（时间有序）
- 事件溯源：每次状态变更调用 `make_event()` + `storage.append_event()`

<!-- MANUAL: -->
