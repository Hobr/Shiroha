# 状态机版本化

> 文档定位：本文只说明“稳定标识与版本边界如何建模”。升级流程与迁移规则见 [升级与迁移](./13-upgrades-and-migrations.md)，in-flight task 的恢复和执行约束见 [执行语义](./11-execution-semantics.md)。

## 稳定标识

- 对外暴露的稳定标识应为 `deployment_id`，而不是 `wasm_hash`
- `deployment_id` 至少由 `machine_name`、`wasm_hash`、WIT / 执行契约版本和能力授权结果共同决定
- 同一 `wasm_hash` 在不同授权结果或不同执行契约版本下，仍应视为不同 deployment

## 版本边界

- 已运行的 instance 和 task 必须继续绑定创建或调度时使用的 `deployment_id`
- 默认流量切换应通过 `release alias` 或等价路由指针完成，而不是原地修改旧 deployment
- 若状态 schema、事件 payload、回调协议或执行契约发生不兼容变化，应创建新 deployment，并由显式迁移流程处理

## 版本协同

- Guest SDK 版本应与 WIT 契约主版本对齐
- Controller / Node 之间必须共享相同的执行契约版本
