# Phase 1 审计清单

这份清单只保留当前尚未完成、且会影响 Phase 1 收口的事项。
已落地的修复项已从这里移除；如需追溯历史修复，请查看 git 历史和 `docs/roadmap.md`。

当前判断原则：

- `按原 Phase 1 计划仍待实现`：`docs/roadmap.md` 里 Phase 1 明确承诺过，但代码还没有完整交付
- `Phase 1 文档与验收收口`：如果短期不补代码，至少要把文档、示例、CLI 提示降到真实现状
- `Phase 1 工程收尾`：不改变执行模型，但会影响 Phase 1 的验收清晰度、测试完整性或后续维护成本

不属于本次 Phase 1 收口的更大能力补齐，统一回到 `docs/roadmap.md` 的 Phase 2+ 跟踪，不再在这里重复展开。

## 按原 Phase 1 计划仍待实现

- [ ] 为 `fan-out` 补上 standalone 运行时分发、结果收集和 `aggregate()` 后的状态推进；当前 deploy 仍直接拒绝 `fan-out` action

## Phase 1 文档与验收收口

- [ ] 把 `remote` 在 standalone 中仍复用本地 WASM 调用路径的事实写清楚，不再暗示已有真实的 Controller/Node 执行边界
- [ ] 把 `fan-out`、`fork`、`join`、自动 `subprocess` 编排当前未完整支持的事实写清楚，不再让文档和示例看起来像已可用能力
- [ ] 收紧 `docs/event-sourcing.md` 中对事件类型和审计范围的描述；当前没有单独的“外部触发事件”记录，也没有 actor/source 信息
- [ ] 收紧 `docs/event-sourcing.md` / `docs/core-concepts.md` 中对 payload、guard 拒绝、排队事件、取消原因等可审计性的暗示
- [ ] 收紧 `docs/wasm-design.md` 中对 capability 粒度的表述；runtime 已按 `action.capabilities` 在每次调用时动态放行 network/storage
- [ ] 收紧 `docs/wasm-design.md` 中对运行时限制的表述；当前只有固定 fuel 预算，`memory_mb` / `timeout_ms` / `max_concurrent` 尚无对应配置面
- [ ] 收紧 `docs/wasm-design.md` 中 `network` world 的安全描述；当前没有 domain allowlist，guest 还可配置 proxy、local address、自定义 root CA 和危险 TLS 选项
- [ ] 收紧 `docs/wasm-design.md` 中 `storage` world 的安全描述；当前拿到 storage 权限的 guest 可以读写任意 namespace，没有 flow/job 级隔离
- [ ] 收紧 `docs/backends.md` 中 “standalone 使用 in-process transport” 的表述；当前 transport 抽象并未进入主执行链路
- [ ] 收紧 `docs/scheduling.md` 中关于 action timeout、retry/backoff、Node 心跳、负载感知、drain、背压的表述；这些能力当前未接入 runtime
- [ ] 收紧 `docs/operations.md` 中 Node 注册、发现、健康检查、优雅停机、OpenTelemetry 指标/追踪的现状表述
- [ ] 明确 `action output` 当前会被 runtime 丢弃，不会持久化，也不会反馈给后续流程
- [ ] 明确 Flow 删除后 `wasm_modules` 与 `module_cache` 当前没有引用计数/共享生命周期策略
- [ ] 统一示例中的平台 `flow_id` 与 guest `manifest.id` 叙事；当前部分 example 在测试里被部署到不同外部 flow_id，会强化身份漂移的默认印象

## Phase 1 工程收尾

- [ ] 以 `docs/roadmap.md` 作为 Phase 1 验收基线，优先完成上面的实现缺口，再处理文档收口项
