# 调度与定时器

## Dispatch Mode

每个 Action 在 manifest 中声明分发模式：

| 模式 | 执行位置 | 说明 |
| ------ | ---------- | ------ |
| `local` | Controller 本地 | 轻量操作，无需网络开销 |
| `remote` | standalone 内的同进程 node worker | 当前已通过 in-process transport 建立 Controller→Node worker 边界 |
| `fan-out` | standalone 内的多个同进程 fan-out 槽位 | 当前会收集 `NodeResult`，再调用 guest `aggregate()` 决定后续事件 |

## 调度策略

当前 Phase 1 里仍没有真正的跨机器节点调度。

补充说明：

- `remote` 当前固定分发到 standalone 内置 node worker
- `fan-out` 的 `all` / `count(N)` / `tagged(...)` 当前只会映射到同进程槽位或标签名，不依赖真实的节点发现与负载信息

以下内容仍属于后续阶段目标：

- 内置策略：round-robin、最小负载、能力标签匹配
- 自定义策略：WASM scheduler plugin
- 同 Flow 的缓存亲和性

## 故障处理

当前 Phase 1 已实现：

- Controller 重启后恢复持久化的 Job 快照、暂停事件队列、timeout 计划、Flow 版本和 WASM 模块缓存
- 恢复边界：当前不会恢复运行中的 in-flight Action，恢复以持久化快照为边界
- fan-out 当前支持整体 `timeout-ms` 和 `min-success` 的提前聚合截断

以下仍属于后续阶段目标：

- 每个 Action 独立 timeout
- 重试次数和退避策略
- Node 下线 / drain

## 背压机制

当前 Phase 1 还没有节点级背压或负载感知调度。

## 定时器

状态机常见需求："如果 N 时间内没有后续事件，自动转移到超时状态"。

### 实现方式

- Controller 维护一个本地定时器管理器；当前实现由集中式 `DelayQueue` 驱动，而不是“一 timer 一 task”
- 状态转移进入某状态时，如果该状态的出边（transition）配置了 timeout，Controller 注册定时器
- 定时器到期后，Controller 向该 Job 的串行处理路径注入 timeout-event
- 对于 transition timeout，Job 暂停时定时器暂停，恢复时重新计算剩余时间
- 对于 job lifetime（`max_lifetime`），采用绝对 wall-clock deadline，不会因 Job pause 而停止计时
- 定时器完全在 Controller 本地处理，不经过 Node

### 使用示例

一个 `waiting_approval` 状态有三条出边：`approved` → 下一状态、`rejected` → 拒绝状态、以及一个 30 分钟的 timeout → 超时状态。进入该状态后，如果 30 分钟内没有收到 `approved` 或 `rejected` 事件，定时器到期自动触发超时事件，驱动状态机转移到超时状态。
