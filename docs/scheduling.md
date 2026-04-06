# 调度与定时器

## Dispatch Mode

每个 Action 在 manifest 中声明分发模式：

| 模式 | 执行位置 | 说明 |
| ------ | ---------- | ------ |
| `local` | Controller 本地 | 轻量操作，无需网络开销 |
| `remote` | 单个 Node | 普通任务分发 |
| `fan-out` | 多个 Node | 并行执行 + 聚合结果 |

## 调度策略

Controller 决定任务发往哪个 Node：

- **内置策略**：round-robin（默认）、最小负载、能力标签匹配
- **自定义策略**：通过 WASM scheduler plugin 实现
- **亲和性**：同 Flow 的任务优先发往已缓存该 WASM 模块的 Node

## 故障处理

- **超时检测**：每个 Action 有 timeout，超时后标记 failed
- **重试策略**：可配置重试次数和退避策略
- **幂等性**：Action 设计必须幂等（fan-out 场景尤其重要）
- **Controller 恢复**：重启后恢复持久化的 Job 快照、暂停事件队列、timeout 计划、Flow 版本和 WASM 模块缓存
- **恢复边界**：当前不会恢复运行中的 in-flight Action，恢复以持久化快照为边界
- **Node 下线**：优雅关机时 drain 正在执行的任务，等待完成或迁移

## 背压机制

Node 过载时 Controller 应减速或排队，避免雪崩。Node 通过心跳上报负载信息，Controller 调度时参考。

## 定时器

状态机常见需求："如果 N 时间内没有后续事件，自动转移到超时状态"。

### 实现方式

- Controller 维护一个定时器轮（hierarchical timer wheel）
- 状态转移进入某状态时，如果该状态的出边（transition）配置了 timeout，Controller 注册定时器
- 定时器到期后，Controller 向该 Job 的串行处理路径注入 timeout-event
- Job 暂停时定时器暂停，恢复时重新计算剩余时间
- 定时器完全在 Controller 本地处理，不经过 Node

### 使用示例

一个 `waiting_approval` 状态有三条出边：`approved` → 下一状态、`rejected` → 拒绝状态、以及一个 30 分钟的 timeout → 超时状态。进入该状态后，如果 30 分钟内没有收到 `approved` 或 `rejected` 事件，定时器到期自动触发超时事件，驱动状态机转移到超时状态。
