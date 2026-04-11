# 传输层与持久化

## 传输层

抽象 Transport trait，初期实现 gRPC，预留接口供未来扩展。trait 形状应优先服务第一条参考实现，不为未来后端预埋复杂抽象：

- gRPC（初期实现）
- QUIC（预留）
- 消息队列（预留）

建议区分两类接口：

- 控制面：模块部署、实例查询、节点注册/心跳、模块拉取
- 任务面：task 下发、结果回传、取消、超时与失败上报

最低要求：

- 任务请求/响应必须包含 `task_id` 与 `deployment_id`
- 节点与 Controller 间至少需要节点身份校验与传输加密
- 节点拉取模块后必须校验 `wasm_hash`

当前建议：

- 先以 gRPC 跑通第一条远程执行闭环，再从已验证路径提炼通用传输抽象
- 在没有真实第二种传输后端之前，不为 QUIC / 消息队列预埋复杂控制流分支

## 持久化

抽象 Storage trait，初期使用嵌入式存储。trait 形状应围绕恢复与审计的最小闭环收敛，再考虑额外后端。持久化内容：

- `deployment_registry`（部署快照与能力授权结果）
- 状态机实例状态快照（含 state schema version 与来源 deployment 信息）
- 事件日志（含 event schema / contract version）
- `task` 与 `task_attempt` 记录
- 节点注册信息与心跳
- WASM 模块 Registry / Blob 索引

恢复语义：

- Controller 重启后应能据持久化状态重建 in-flight task
- 超过租约时间仍无回报的运行中 attempt，应被视为丢失并进入重试或失败决策
- 详细恢复入口与 replay 约束以 [执行语义](./11-execution-semantics.md) 为准
