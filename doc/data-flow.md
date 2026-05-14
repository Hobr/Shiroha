# 端到端数据流

本文以五条典型路径串起所有模块,确认它们的协作面是否对齐。涉及的子系统细节见各自文档。

---

## 1. 上传 Flow

1. `sctl upload-flow <wasm-file>` 通过控制面 gRPC 把字节传给 `shirohad`
2. `shiroha-control` 把请求转发给 Engine 的 "register flow" 命令
3. Engine 调用 `shiroha-wasm` 加载组件,读取 `describe` 导出,得到 FSM 描述符
4. Engine 用 core 的 FSM 校验规则验证描述符合法性
5. Engine 派生 ComponentId(内容 hash);Storage 在**单事务**内:若 ComponentId 已存在则只追加 Flow 引用,否则写入 Component 字节;同事务内写入 Flow 记录(引用 ComponentId)与能力声明
6. 写入完成;**不主动推送到节点**——节点会在首次需要执行该 ComponentId 上的 Action 时按需 pull(见 `worker.md`)
7. 返回给 sctl 的响应包含 FlowId + 版本号

任何一步失败都不允许留下半完成的 Flow——校验失败时,Storage 中无任何残留;字节分发失败不阻断上传成功(节点会在被需要时按需 pull,或下次心跳重试预加载)。

---

## 2. 创建 Job

1. `sctl create-job <flow-id> [--input ...]` 发送至控制面
2. Engine 在事务内写入初始 Job 记录与 "Created" Event
3. Engine 触发初始状态的 on-enter Action 列表
4. 若任一初始 Action 硬失败,Engine **回滚**:删除 Job 与已写 Event;返回错误
5. 若初始 Action 全部成功,Job 进入 Running,等待第一次转移触发条件
6. sctl 收到 JobId

回滚是显式动作,不依赖事务自动撤销——因为 Action 派发已经发生,可能产生外部副作用,Engine 必须留下明确的"已尝试启动并失败"事件。

---

## 3. 一次状态转移

1. 触发条件命中(外部输入 / 内部定时器 / 上一 Action 完成)
2. Engine 读取当前状态的 on-exit Action 列表
3. 每个 Action 经 Dispatcher:
   - DispatchPolicy 决定执行者集合
   - LocalExecutor 或 RemoteExecutor 执行(透传 ComponentId 给 Executor)
   - 多结果经 Aggregator 收敛为单一结果
   - 若 ActionRef.WaitingMode 为 `Waiting`,Engine 在派发瞬间把 Job 切到 `Waiting` 中间态(同事务写状态 + 事件),释放驱动循环;结果回流后再继续;`Blocking` 模式下 Engine 原地 await
4. 转移决策 Action 经 Dispatcher 执行,产生决策输入
5. Engine 调用 WASM `decide` 得到下一状态
6. **单事务**写入:新状态 + Transition Event
7. Engine 触发新状态的 on-enter Action
8. 若新状态为终态,Job 标记完成;否则等待下一触发

每一步的失败都不允许留下半成品状态。失败模式详见 `engine.md` 与 `storage.md`。

---

## 4. 节点离线与恢复

1. 心跳超时,Transport 通知 NodeRegistry
2. NodeRegistry 标记节点为不可用
3. Dispatcher 在下一次选择时跳过该节点
4. 正在该节点上的未完成 Action 由 Aggregator 视为失败(具体处置取决于聚合策略)
5. 节点重新上线后向主控声明,NodeRegistry 恢复其可用标记
6. 主控不主动补推 WASM 字节;节点再次接到 Action 请求时按需 pull

节点离线不应导致 Job 失败,除非 DispatchPolicy 已无足够目标(如 Single 节点选择器只指向了它,或 Quorum 凑不齐 k 个)。

---

## 5. 主控重启

1. `shirohad` 启动,加载配置
2. Engine 从 Storage 读取所有未终态 Job
3. 每个 Job 根据其最后一次 Event 重建驱动循环
4. 定时器(Action 超时、状态停留超时、Job 寿命超时)从 Storage 快照重建
5. 已经派发但未持久化结果的 Action(Blocking 模式):Engine 向相关节点查询 `action_id` 的缓存结果(节点按 Q6 决议保留近期结果数分钟);命中则恢复执行,过期则视为失败并走 ActionRef 失败路径。Waiting 模式下则直接订阅结果通道
6. 控制面与节点面 RPC 服务启动,接受连接
7. NodeRegistry 等待节点上报心跳;在节点恢复前,Dispatcher 对依赖该节点的 Action 视为不可用

恢复必须保持幂等:重启途中再次崩溃,不会引入新的状态紊乱。

---

## 跨流程不变量

- 状态变更与对应事件**永远**在同一事务中落盘
- 主控之外没有任何"权威状态"
- 节点之间不通信
- WASM 主机能力只通过受控 import 提供,不存在旁路
