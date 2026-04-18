# 可观测性

一次事件跨越 CLI → 主控 → Transport → Agent → WASM，缺失统一追踪则无法排障。约定如下。

## 标识

- `event_id`: 由主控在 `SubmitEvent` 入队时生成 (uuid v7)，贯穿整次事件处理。
- `event_seq`: 实例维度单调递增序号，用于事件日志游标与续传。
- `attempt_id`: 每次 Action 调用的稳定键，组成见 [semantics](05-semantics.md)。
- `trace_id` / `span_id`: 遵循 W3C TraceContext；若客户端传入则沿用，否则主控生成。

## 传播路径

- **客户端 → 主控**: gRPC metadata 携带 `traceparent`。
- **主控 → 节点**: Transport 调用在请求头注入 `traceparent`、`event_id`、`attempt_id`。
- **节点 → WASM**: 通过 WIT 的 logging interface 把当前上下文暴露给 guest；guest 的日志自动附加这些字段。
- **异步回调 / 流**: 事件流返回时回填相同 trace 上下文，便于订阅侧关联。

## 日志

- 使用 `tracing` 作为统一入口；主控、节点、CLI 共享一份订阅器初始化代码 (位于 `shiroha-config` 或独立工具 crate)。
- 关键 span: `event.handle`、`action.dispatch`、`action.invoke`、`action.aggregate`、`state.commit`。
- 日志默认 JSON 输出，生产环境接入集中式日志系统。

## 指标

建议的最小指标集：

- `shiroha_events_total{instance, outcome}`
- `shiroha_action_duration_seconds{action, node, outcome}`
- `shiroha_dispatch_fanout{action, plan}`
- `shiroha_aggregate_outcome{action, spec}`
- `shiroha_nodes_registered`
- `shiroha_storage_commit_duration_seconds`
- `shiroha_wasm_fuel_consumed{action}`

指标输出由 `shiroha-controller` / `shiroha-node` 各自暴露；当前版本不绑定具体后端。

## 审计

事件日志自身即审计来源 (不可变、按 seq 追加)。主控 gRPC 的写操作 (`DeployFsm`、`CreateInstance`、`SubmitEvent`) 需额外记录调用者身份 (见 [security](11-security.md))。
