# Phase 1 审计清单

这份清单用于严格对照当前实现与 Phase 1 目标，记录已经确认的缺口、半完成能力和需要回收的文档表述。

## Runtime Correctness

- [ ] 让多条 `(from, event)` 候选转移真正按 guard 选择可行边，而不是先固定第一条再决定成功/失败
- [ ] 为 `create_job` 的初始 `on-enter` 失败定义原子语义：要么回滚 Job 创建，要么在错误响应中返回已提交的 `job_id`
- [ ] 为 `trigger_event` / 转移后 action 失败定义清晰契约：当前会在状态转移已经提交后再返回 `aborted`
- [ ] 明确 `fork` / `join` 状态类型在运行时的语义；若本阶段不支持，应在 deploy 时拒绝
- [ ] 明确 `subprocess` 状态在 Phase 1 的允许范围；若只允许“可声明不可执行”，需要把这条语义收紧到一致的文档和校验
- [ ] 避免 network capability action 直接阻塞 tokio worker；当前 host 走 `reqwest::blocking`，且 action 调用路径没有 `spawn_blocking` 隔离

## Deploy Validation

- [ ] 在 deploy 期逐个验证 manifest 中命名的 `action` 是否被 guest 真正支持，而不只是存在通用 `invoke-action` 导出
- [ ] 在 deploy 期逐个验证 manifest 中命名的 `guard` 是否被 guest 真正支持，而不只是存在通用 `invoke-guard` 导出
- [ ] 在 deploy 期验证 `fan-out.aggregator` 名称是否被 guest 真正支持
- [ ] 为 `kind = subprocess` 增加静态校验，确保 `subprocess.flow-id` / `completion-event` 必填
- [ ] 为当前 runtime 尚不支持的 shape 增加前置拒绝策略，例如 `fan-out` action、未实现语义的状态类型
- [ ] 决定并落实 `DeployFlowRequest.flow_id` 与 `manifest.id` 的一致性策略；当前两者可以不一致

## Execution Model

- [ ] 决定 `remote` 在 Phase 1 的目标语义：实现真实的 in-process Controller/Node 边界，或明确把它降级为 `local` 别名
- [ ] 若 `InProcessTransport` / `Scheduler` 只保留骨架，则把它们从“当前能力”调整为“预留抽象”；若要算当前能力，则需要真正接入 runtime
- [ ] 为 `fan-out` 选择明确策略：要么在 Phase 1 拒绝 deploy，要么补齐可执行路径；不要继续保持“可部署不可运行”

## Event And API Contracts

- [ ] 决定事件日志是否需要记录“外部触发事件本身”；当前只记录创建、转移、action 完成和生命周期事件
- [ ] 决定事件日志是否需要记录 guard 拒绝、未知事件、暂停期间排队、取消原因等失败/控制面输入
- [ ] 若“审计追踪”是 Phase 1 目标，补充 actor / caller / source 元数据；当前事件模型无法支持“谁在什么时候触发了什么操作”
- [ ] 决定是否需要持久化触发时的 payload 摘要或引用；当前 event log 中无法查询输入 payload

## Storage And Persistence

- [ ] 修正 latest Flow alias 语义；当前 `save_flow` 在 `MemoryStorage` 和 `RedbStorage` 中都是“最后一次写入覆盖”，并不保证 `get_flow` / `list_flows` 返回版本号最大的注册版本
- [ ] 为 Flow 删除补齐原始 WASM 字节清理；当前删除 flow 只删 latest alias 和 version history，不删 `wasm_modules` 表里的孤儿字节
- [ ] 为 Flow 删除补齐进程内 module cache 清理；当前 `flow_registry.remove_flow()` 不会移除 `module_cache` 中已编译的 component
- [ ] 明确 `save_job_with_event` 的原子性契约；当前 `Storage` trait 默认实现是“先 save_job 再 append_event”，只有覆写的后端才真正单事务提交

## Query Semantics

- [ ] 统一 `get_events` 的排序契约；当前 `MemoryStorage` 返回追加顺序，`RedbStorage` 返回按 `timestamp_ms` 排序，跨后端语义不完全一致
- [ ] 为事件排序补充稳定 tie-breaker；当前 `RedbStorage` 只按 `timestamp_ms` 排序，同毫秒多事件时 `since_id` 游标语义可能不稳定
- [ ] 决定 `GetJobEvents` 的 cursor 语义是否应基于“稳定事件序列”而不是当前后端自定义顺序
- [ ] 修正“列出所有 Job”的发现路径；当前客户端通过 `list_flow_ids()` 聚合 `list_jobs_for_flow()`，一旦 latest Flow alias 缺失或回退，`--all` 视图会漏 Job
- [ ] 修正 follow 模式的 cursor 推进；当前客户端会对服务端返回事件重新排序，再把“重排后的最后一个 id”作为下一轮 `since_id`，服务端与客户端排序不一致时可能跳事件或重复事件

## Client And CLI Semantics

- [ ] 明确 `force_delete_job` / `force_delete_flow` 的非原子语义；当前是逐步 cancel/delete 的顺序操作，中途失败会留下部分修改
- [ ] 解除 `sctl job wait --state` 的歧义；当前同一个参数同时匹配 lifecycle state 和 `current_state`，若业务状态名与生命周期名冲突会提前命中

## Observability

- [ ] 决定 `tracing 结构化日志` 的验收标准；若要求结构化输出链路，当前默认 `fmt()` 文本日志还不够
- [ ] 决定是否需要给 Job lifetime 超时取消增加显式原因记录，而不是只落一个通用 `Cancelled`

## WASM Runtime Limits

- [ ] 收敛或实现 `docs/wasm-design.md` 中声明的运行时限制配置项；当前只有固定 fuel 预算，`memory_mb` / `timeout_ms` / `max_concurrent` 尚无对应配置面
- [ ] 明确当前 capability 模型到底是 component 级还是 action 级；runtime 已按 `action.capabilities` 在每次调用时动态放行 network/storage，但文档仍把更细粒度约束写成后续迭代
- [ ] 决定 action `output` 的去向；当前 guest 可以返回 output，但 runtime 既不持久化，也不暴露给上层流程
- [ ] 为 `network` capability 落实文档里声明的 domain allowlist，或把文档降级为“当前无域名白名单限制”

## Documentation Reconciliation

- [ ] 收紧 `docs/scheduling.md` 中关于 action timeout、retry/backoff、Node 心跳、负载感知、drain、背压的表述；这些能力当前未接入 runtime
- [ ] 收紧 `docs/event-sourcing.md` 中对事件类型和审计范围的描述；当前没有单独的 `Event` 记录，也没有 actor 信息
- [ ] 收紧 `docs/operations.md` 中 Node 注册、发现、健康检查、优雅停机、OpenTelemetry 指标/追踪的现状表述
- [ ] 收紧 `docs/wasm-design.md` 中对 `remote`、`fan-out`、`fork`、`join`、`subprocess` 支持面的说明
- [ ] 收紧 `docs/backends.md` 中 “standalone 使用 in-process transport” 的表述；当前 transport 抽象并未进入主执行链路
- [ ] 收紧 `docs/core-concepts.md` / `docs/wasm-design.md` 中对 `fork` / `join` 状态类型的隐含支持预期；当前只有 schema 形状，没有运行时语义
- [ ] 修正文档里对定时器实现的性能/结构暗示；当前实现是一计时器一 `tokio::sleep` 任务，不是 hierarchical timer wheel
- [ ] 收紧 `docs/wasm-design.md` 中对 capability 粒度和运行时限制的表述，使其与当前 host 实现一致

## Suggested Tests

- [ ] 增加“同一 `(from, event)` 下多候选转移按 guard 选边”的失败用例
- [ ] 增加“初始 `on-enter` 失败时 create-job 的可见语义”测试
- [ ] 增加“转移已提交但 action 失败时 trigger-event 的可见语义”测试
- [ ] 增加“`fan-out` flow 在 deploy 期被拒绝或被明确标记 unsupported”测试
- [ ] 增加“`kind = subprocess` 但缺少 subprocess 配置”的 deploy 校验测试
- [ ] 增加“旧版本后写入时 latest alias 不能回退”的存储测试
- [ ] 增加“删除 Flow 会清理 wasm bytes / module cache”的测试
- [ ] 增加“同毫秒多事件时 `GetJobEvents.since_id` 仍稳定”的查询测试
