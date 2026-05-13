# Action 分发与聚合 (shiroha-dispatch)

## 角色

`shiroha-dispatch` 是 Engine 与"具体执行点"之间的中间层。它接收一个 ActionRef + 输入,根据 core 中声明的 DispatchPolicy 选择执行者集合,执行并收集结果,然后用 Aggregator 合并,把单一结果交还给 Engine。

Engine 不知道 Action 是本地还是远程执行,也不知道有几个 Worker 参与——这两件事都被 dispatch 层吸收。

## 抽象层次

```
Engine
   │
   ▼
Dispatcher        ← 按 DispatchPolicy 选 executor 集合,驱动 Aggregator
   │
   ├──▶ LocalExecutor   ── 直接调用 shiroha-wasm
   │
   └──▶ RemoteExecutor  ── 通过 shiroha-transport 派发到节点
```

`LocalExecutor` 与 `RemoteExecutor` 实现同一个执行接口,只是位置不同。

## DispatchPolicy → Executor 映射

| Policy | Executor 集合 | Aggregator 必要性 |
|---|---|---|
| Local | 单个 LocalExecutor | 不需要 |
| Single(selector) | 单个 RemoteExecutor(节点由选择器决定) | 不需要 |
| Fanout(n, agg) | n 个 RemoteExecutor(由选择器选出) | 必需 |
| Broadcast(agg) | 所有已注册节点的 RemoteExecutor | 必需 |

主控自身可被节点选择器选中,从而把"本地路径"作为 Fanout/Broadcast 的成员之一;此时由 LocalExecutor 承担,避免一次自环 RPC。

## Aggregator 接入点

Aggregator 是同步语义的"收集器":接收若干结果,可能在收够最低数量时提前返回(如 First / Quorum)。Dispatcher 必须把"提前返回"和"剩余执行如何处置"一并定义。

剩余执行的处置策略,见 `open-questions.md`:

- 取消(沿 transport 下发取消信号)
- 后台等待(不取消但不阻塞主路径)
- 忽略(简单但浪费资源)

## 自定义聚合

当 ActionRef 声明 `Aggregation::Custom` 时,Dispatcher 收齐结果后回调 WASM 的 aggregate export(见 `wit-interfaces.md`)。自定义聚合不能阻塞 Dispatcher;若超时,Dispatcher 视为聚合失败。

## 失败传播

- Executor 内部任何失败统一上抛为 `ExecutionError`,区分网络型与业务型
- Aggregator 决定 ExecutionError 是否构成整体失败(如 AllOk 中任一失败即整体失败)
- Dispatcher 把最终的整体结果或失败交回 Engine;Engine 根据 ActionRef 元数据决定下一步(转入失败状态 / 触发补偿 / 重试)

补偿与重试策略由用户在 FSM 定义中声明,Engine 调度;Dispatcher 不内置任何业务重试。

## 与其他 crate 的契约

- 入参:`shiroha-core` 的 ActionRef + 输入字节
- 出参:聚合后的结果字节 或 ExecutionError
- 依赖:`shiroha-wasm` 提供 LocalExecutor,`shiroha-transport` 提供 RemoteExecutor

## 不在本 crate 内的内容

- 实际网络协议(在 transport)
- WASM 执行细节(在 wasm)
- 状态持久化(在 storage)
- Job 生命周期(在 engine)

Dispatcher 只回答一个问题:"给我一个 Action + 输入,你给我聚合好的结果"。
