# 核心概念

## Flow

一个完整的状态机定义，对应一个 WASM 模块。包含：

- **State**：状态节点，可附带 `on-enter` / `on-exit` Action
- **Transition**：状态转移边，包含触发事件、Guard 条件、Action
- **Action**：可执行的业务逻辑函数
- **Guard**：转移前的条件判断函数（必须是纯函数、确定性）

在当前 standalone 实现里，状态级 hook 已经会被真正执行：

- 创建 Job 后，会执行初始状态的 `on-enter`
- 状态转移时，按 `on-exit -> transition action -> on-enter` 的顺序执行
- 这些执行结果和普通 Action 一样会记录到事件日志里

## Job

一个 Flow 的运行实例。绑定特定版本的 Flow WASM 模块。

### Job 生命周期

```
            create
              │
              ▼
           running ◄──── resume
           │  │  │
  timeout/  │  │  └──── pause ────► paused
  cancel    │  │
           │  ▼
           │  terminal state
           │  │
           ▼  ▼
        cancelled  completed
```

| 状态 | 说明 |
|------|------|
| `running` | 正常运行，响应事件和定时器 |
| `paused` | 暂停，不响应事件（事件入队但不处理），不触发定时器 |
| `cancelled` | 强制终止，清理本地 timeout 和暂停期间暂存事件 |
| `completed` | 状态机到达终态，正常结束 |

操作接口：

- `pause(job_id)`：暂停 Job，外部事件和定时器事件入队但不推进状态机
- `resume(job_id)`：恢复 Job，按序处理暂存的事件
- `cancel(job_id)`：取消 Job，并清理该 Job 相关的本地定时器
- Job 级别可配置 `max_lifetime`，超时后由 Controller 自动取消

### Job 并发控制

同一个 Job 可能同时收到多个事件（外部触发、定时器到期、Execution 完成回调）。当前 Phase 1 的正确性保证来自 **Job 级串行锁 + 暂停期间事件队列**：

- Controller 内部每个 Job 同一时刻只有一个事件在处理
- `running` 状态下，事件在持有该 Job 锁后立即处理
- `paused` 状态下，事件会持久化到 Job 快照中的队列，`resume` 后按顺序回放
- 当前还没有一个“所有状态通用、持久化的 FIFO event inbox” 抽象
- 事件处理过程：获取 Job 锁 → 评估 Guard → 提交状态转移 → 执行 `on-exit` / transition action / `on-enter`

这是正确性保证的基础，避免并发状态转移导致状态不一致。

### 版本迁移

- 新版 Flow WASM 部署后，已运行的 Job 继续使用创建时绑定的旧版 WASM
- 新创建的 Job 使用最新版本
- 当前实现会同时持久化最新版本别名、版本历史和原始 WASM 字节
- Controller 重启后会重建模块缓存，并按 Job 快照恢复暂停事件队列和 timeout 计划，因此旧 Job 可以继续按绑定版本运行
- 当前重启恢复不恢复运行中的 in-flight Action；恢复边界以持久化的 Job 快照为准
- “旧版 WASM 自动清理/保留策略”仍属于后续迭代

## Execution

一次 Action 的执行。可能在本地、远程单节点、或多节点并行执行。

## 子流程（Subprocess）

一个状态可以触发另一个完整的 Flow 作为子流程：

```
主 Flow Job-1:  init ──► [审批] ──► done
                            │  ▲
                    启动子 Flow │  │ 完成回调
                            ▼  │
子 Flow Job-2:          start ──► review ──► approved
```

设计目标：

- 主 Flow 中 `state-kind = subprocess` 的状态进入时，Controller 自动创建子 Job
- 子 Job 完成后，Controller 向主 Job 注入 `completion-event` 驱动继续
- 子 Job 取消/失败时，可配置是否级联取消主 Job
- Controller 维护父子 Job 关联关系，支持查询子 Job 状态

当前实现状态：

- `subprocess` manifest 声明已经可部署和查询
- 自动创建子 Job、父子关联管理、完成回注仍未落地
- 现阶段可通过手工触发 `completion-event` 的方式模拟子流程回注
