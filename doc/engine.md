# 主控引擎 (shiroha-engine)

## 角色

引擎是主控的核心。它消费控制面的命令,驱动 Job 的状态转移,把 Action 委托给 Dispatcher,把状态变更与事件落入 Storage。

引擎**不直接接触** WASM 字节、网络、磁盘——所有外部交互都通过 trait(Dispatcher / Store)。这是 Engine 能保持单元可测的关键。

## 主要职责

- 接收上传 Flow 的命令:派生 ComponentId(内容 hash)→ 加载组件 → 验证 FSM 结构 → 在 Storage 中去重写入 Component 字节并登记 Flow 版本
- 接收创建 Job 的命令:初始化 Job → 写入初始状态与事件 → 触发首批 Action
- 接收 delete-flow 命令:无 `--force` 时,若存在非终态 Job 引用此 Flow 则拒绝;`--force` 时先取消所有相关非终态 Job 再删除 Flow
- 推动 Job 从一个状态转移到下一个状态
- 在终态时停止驱动并标记完成
- 在失败时按 FSM 声明执行重试 / 补偿 / 进入失败状态
- 主控启动时,从 Storage 中重建未终态 Job 的驱动循环
- 应用 Job 跨版本迁移(显式命令触发,详见 `storage.md` 的"版本演进与 Job 迁移"节)

## Job 生命周期

```
Created ──▶ Running ──▶ (Waiting?) ──▶ Running ──▶ ... ──▶ Terminal
                │                                              ▲
                └──── Failed / Cancelled ──────────────────────┘
```

是否存在显式 `Waiting` 中间态(配合长 Action 的异步回调)见 `open-questions.md`。无论同步还是异步模式,以下不变量都必须满足:

- 任意时刻 Job 的状态都能从 Storage 中查询到
- 进入下一状态前,本次转移的 Event 必须已经持久化
- 节点离线 / 重启 / 崩溃后,Engine 能从 Storage 中恢复全部未终态 Job

## 一次状态转移的步骤

1. 读取当前状态的 on-exit Action 列表
2. 依次或并发(取决于 FSM 声明)委托给 Dispatcher 执行
3. 读取转移决策 Action,委托 Dispatcher 执行,得到决策输入
4. 调用 WASM 的 `decide` 得到下一状态
5. 在**同一事务**内:写入新状态 + append 转移 Event
6. 触发新状态的 on-enter Action 列表
7. 若新状态为终态,标记 Job 完成;否则等待下一个触发条件

任意一步失败,都必须把已经发起但未确认的 Action 处置妥当(取决于 Aggregator 的剩余处置策略,见 `dispatch.md`),并把 Job 留在一个 Storage 中可恢复的状态。

## Action 的两种执行路径

每个 Action 派发时,Engine 根据 ActionRef 的 **WaitingMode**(见 `core-model.md`)选一条路径:

- **Blocking** — Engine 在第 2/3 步原地 await Dispatcher 的结果;Job 在 Storage 中仍处转移源状态,直到第 5 步落盘新状态。事件流紧凑,但长 Action 期间外部观测看不到独立中间态
- **Waiting** — 派发瞬间在同一事务内写入"Job 进入 `Waiting(action_id)`"事件与状态;Engine 释放当前驱动循环的控制权;结果回流(由 Dispatcher 通过结果通道告知 Engine)时,Engine 唤醒该 Job 并继续后续步骤(决策 → 写新状态 → on-enter)

无论哪种模式,从一次转移开始到完成,Storage 中的 Job 状态必须始终是可恢复的快照:主控随时崩溃,重启后都能从 Storage 推算出"上一次到哪一步"。Waiting 模式下,重启后 Engine 可以直接看到 `Waiting(action_id)` 并重新订阅该 action 的结果通道。Blocking 模式下,Engine 通过事件日志查到尚未完成的 dispatch 记录后,**向相关节点查询 `action_id` 的缓存结果**(节点按 Q6 决议保留近期结果数分钟,见 `worker.md`);命中则继续推进,过期则该 Action 视为失败,走 ActionRef 声明的失败处理路径。

## 失败与回滚

- 创建 Job 时若初始 Action 硬失败,已写入的 Job 与 Event 须被回滚(参考此前实现中的 rollback helper 思路)
- 转移过程中失败,Engine 不允许把"半个状态"留在 Storage
- 用户 FSM 中声明的重试与补偿由 Engine 调度,但**逻辑由 WASM 决定**——Engine 不内置业务重试策略

## 调度模型

- 每个 Job 在 Engine 内是一个独立的驱动循环
- 多 Job 之间并发;单 Job 内的状态转移**串行**(状态机语义本身的要求)
- 暂停 / 恢复:Job 可被显式 pause,pause 期间不消费新触发;恢复时从最近一次持久化状态继续
- 取消:cancel 是不可逆操作,Job 直接进入终态 `Cancelled`,并执行 FSM 声明的 cancel 钩子(若有)

## 超时与定时器

Engine 维护一组定时器,用于:

- 单 Action 的执行超时
- 状态级别的停留超时(用户在 FSM 中可声明 "在状态 X 停留超过 T 触发转移")
- Job 总寿命超时

主控重启后,这些定时器从 Storage 中的快照重建(参考此前 `JobService rebuilds timeout schedules after state transitions` 的语义)。

## 与控制面的边界

Engine 不直接暴露 RPC;所有外部命令通过 `shiroha-control` 中的服务转发进 Engine。这避免 Engine 因控制面协议变化而被迫修改,也让控制面可被替换(将来 TUI / Web 客户端共用同一 Engine 入口)。

## 与其他 crate 的契约

- 依赖:`shiroha-core`、`shiroha-wasm`、`shiroha-dispatch`、`shiroha-storage`
- 被依赖:`shiroha-control`(转发命令)、`apps/shirohad`(装配)
- 不依赖:不直接依赖 `shiroha-transport`(经由 dispatch)、不依赖 `shiroha-proto`
