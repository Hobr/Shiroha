# 核心概念

## Flow

一个完整的状态机定义，对应一个 WASM 模块。包含：

- **State**：状态节点，可附带 `on-enter` / `on-exit` Action
- **Transition**：状态转移边，包含触发事件、Guard 条件、Action
- **Action**：可执行的业务逻辑函数
- **Guard**：转移前的条件判断函数（必须是纯函数、确定性）

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
| `cancelled` | 强制终止，清理 in-flight Execution |
| `completed` | 状态机到达终态，正常结束 |

操作接口：

- `pause(job_id)`：暂停 Job，in-flight Execution 继续完成但结果暂存不推进状态机
- `resume(job_id)`：恢复 Job，按序处理暂存的事件和 Execution 结果
- `cancel(job_id)`：取消 Job，通知相关 Node 取消正在执行的 Execution
- Job 级别可配置 `max_lifetime`，超时自动取消

### Job 并发控制

同一个 Job 可能同时收到多个事件（外部触发、定时器到期、Execution 完成回调）。采用**串行化**模型：

- 每个 Job 持有一个有序的 event inbox（FIFO 队列）
- Controller 内部每个 Job 同一时刻只有一个事件在处理
- 事件处理过程：取出事件 → 评估 Guard → 执行状态转移 → 触发 Action → 完成后取下一个事件

这是正确性保证的基础，避免并发状态转移导致状态不一致。

### 版本迁移

- 新版 Flow WASM 部署后，已运行的 Job 继续使用创建时绑定的旧版 WASM
- 新创建的 Job 使用最新版本
- 旧版 WASM 在所有关联 Job 完成后可清理

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

- 主 Flow 中 `state-kind = subprocess` 的状态进入时，Controller 自动创建子 Job
- 子 Job 完成后，Controller 向主 Job 注入 `completion-event` 驱动继续
- 子 Job 取消/失败时，可配置是否级联取消主 Job
- Controller 维护父子 Job 关联关系，支持查询子 Job 状态
