# 节点端 (shiroha-worker)

## 角色

节点是**无状态执行器**。它向主控提供执行能力,接收主控通过节点面 transport 下发的 Action 调用请求,在本地 WASM 运行时中执行,并把结果回报给主控。

节点上不持有任何 Job 状态。组件缓存与短期 Action 结果缓存都是性能优化,丢失不影响正确性:组件按需重 pull;结果缓存过期会让主控重启恢复 Blocking 模式 Action 时把它视为失败(详见 `engine.md`)。

## 主要职责

- 启动时向主控声明自己(endpoint、能力标签、版本信息)
- 维持组件缓存:按 `ComponentId`(内容 hash)缓存 WASM 字节;不感知 Flow 概念
- 接收 Action 调用请求,实例化(或复用)WASM 组件,调用对应 export
- 把结果或错误回传主控
- 维持短期 Action 结果缓存:按 `action_id` 缓存最近完成的结果数分钟,供主控重启后查询取回(支持 Blocking 模式恢复,见 `engine.md`)
- 响应取消信号,中断正在进行的 WASM 调用(受 Wasmtime 取消能力限制,best-effort)
- 在主控不可达时退化为"等待重连",不做任何自主决策

## 组件缓存策略

节点对每个 `ComponentId` 缓存一份 WASM 字节(主控在请求里传 ComponentId,而非 Flow 版本)。

**采用按需 pull**:节点收到 Action 请求时,若本地无该 ComponentId 的缓存,通过节点面 transport 向主控发起一次拉取,得到字节后实例化并执行;后续请求命中缓存即可。

主控始终是字节的权威源,节点缓存可丢可重建,重启后缓存为空属于正常态。缓存淘汰由节点自行决策(LRU / 容量上限),不向主控通报。

## 执行模型

- 节点内部维护 Wasmtime engine 与按需创建的 component instance
- Action 调用是无副作用的纯函数(从节点的角度看):同一 ActionRef + 输入应产生同样的结果(忽略 WASM 内部使用主机能力如时钟造成的差异)
- 单个节点上的多个 Action 调用并发执行;调用之间不共享内存(除非走 host KV 能力)
- Action 执行时间上限由主控通过请求元数据传入;超时由节点本地强制

## 与主控的边界

节点端**只调用** WASM 中"Action 实现"相关的 export(见 `wit-interfaces.md`),不调用 `describe` / `decide`。这两个 export 仅在主控加载组件时使用。这样即使 WIT 演进出主控专用 export,节点端 binary 也不需要更新。

节点不知道 Job 的存在;它只知道"有一个 Action 被请求执行了"。如果业务需要 Action 知道自己属于哪个 Job,主控通过请求元数据传入,而非通过节点查询。

## 心跳与健康检查

节点通过节点面 transport 周期性上报心跳。主控的 NodeRegistry 根据心跳决定节点是否"健康"。心跳协议细节属于具体 transport 实现,但 NodeRegistry 对外提供统一的健康状态查询接口给 Dispatcher。

节点离线后再上线不需要重新"加入集群"——它只是再次开始心跳;主控 NodeRegistry 恢复其可用标记。

## 与主控复用同一二进制

主控 `shirohad` 与节点 `shirohad` 是同一二进制,通过启动参数切换模式。主控模式启动时可同时启用 worker 角色,把本机作为一个节点注册进自己的 NodeRegistry。

这种"自托管 worker"路径走 LocalExecutor 而非 RemoteExecutor,避免一次自环 RPC(见 `dispatch.md`)。从用户视角看,FSM 定义中的 DispatchPolicy 不需要为此特殊处理——是否走本地路径由主控装配层决定。

## 与其他 crate 的契约

- 依赖:`shiroha-core`(NodeId 等)、`shiroha-wasm`、`shiroha-transport`(服务端)
- 不依赖:`shiroha-engine`、`shiroha-storage`、`shiroha-dispatch`
- 入口:接收 transport 的请求 → 调用 wasm 的 Action API → 回应 transport
