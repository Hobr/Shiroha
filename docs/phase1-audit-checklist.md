# Phase 1 审计清单

这份清单按建议处理顺序重排：

- `Phase 1 必修`：不修会让当前 MVP 语义不自洽，或直接误导用户/调用方
- `Phase 1 文档收口`：如果短期不改代码，至少要把文档、示例、CLI 提示降到真实现状
- `后续阶段再做`：更大的能力补齐、治理或工具链完善工作

## Phase 1 必修

- [x] 让多条 `(from, event)` 候选转移真正按 guard 选择可行边，而不是先固定第一条再决定成功/失败
- [x] 修正 `create_job` 的初始 `on-enter` 语义：不要在 Job 已创建后再返回 `aborted` 且不返回 `job_id`
- [x] 修正 `trigger_event` / 转移后 action 失败的 RPC 语义：不要在状态转移已提交后仍对调用方表现为原子失败
- [ ] 明确 timeout 的绑定语义；当前 runtime 只会注入 `timeout_event` 再走普通事件匹配，不会把 timeout 绑定回声明它的那条 transition
- [x] 补齐 manifest 基础合法性校验：当前不会拒绝重复 state/action 名称、空字符串标识、`timeout.duration_ms = 0`、`FanOutStrategy::Count(0)`、空 `aggregator` / `timeout_event` 等明显无效配置
- [x] 为 timeout 配置增加静态约束，确保 `timeout_event` 在同一源状态上匹配到唯一且明确的转移
- [x] 为当前 Phase 1 不支持的 shape 增加 deploy 期前置拒绝策略，例如 `fan-out` action、`fork` / `join`、未落地的 `subprocess` 运行时语义
- [x] 修正 latest Flow alias 语义；当前 `save_flow` 在 `MemoryStorage` 和 `RedbStorage` 中都是“最后一次写入覆盖”，并不保证 `get_flow` / `list_flows` 返回版本号最大的注册版本
- [x] 统一“latest Flow”在查询与执行路径中的来源；当前 `GetFlow` / `ListFlows` 读 storage latest alias，而 `CreateJob` 读内存 `flow_registry.latest_registration()`
- [x] 统一 `GetJobEvents` 的服务端排序契约，并在过滤/`limit`/cursor 之前先按稳定顺序排序
- [x] 为事件排序补充显式 tie-breaker；当前 `RedbStorage` 只显式按 `timestamp_ms` 排序，等 timestamp 的相对顺序没有写进契约
- [x] 修正 follow 模式的 cursor 推进；当前客户端会对服务端返回事件重新排序，再把“重排后的最后一个 id”作为下一轮 `since_id`
- [x] 修正 `follow --tail N` 的批次语义；当前实现会对每一批新事件都截尾，不只是首批历史事件
- [ ] 收紧“列出所有 Job”的发现路径；当前客户端通过 `list_flow_ids()` 聚合 `list_jobs_for_flow()`，若 flow 清单与 job 实际集合失配，`--all` 视图会漏 Job
- [ ] 解除 `sctl job wait --state` 的歧义；当前同一个参数同时匹配 lifecycle state 和 `current_state`
- [ ] 决定 `CreateJobRequest.context` 的真实角色；当前它会持久化并在 API 中暴露字节长度，但不会传给 guest action/guard，也没有读取接口
- [ ] 给 `GetJob` / CLI job 展示补齐 lifetime 可观测性；当前用户可以创建 `max_lifetime_ms`，但后续查询看不到配置值、deadline 或剩余时间
- [ ] 避免 network capability action 直接阻塞 tokio worker；当前 host 走 `reqwest::blocking`，且 action 调用路径没有 `spawn_blocking` 隔离
- [ ] 收敛重启恢复的失败域；当前只要有一个持久化 Flow 版本缺少 wasm bytes 或无法重新编译，整个 `shirohad` 启动都会失败
- [ ] 为 `job_locks` 增加生命周期管理；当前锁只在 `delete_job` 时移除，终态但未删除的 Job 会让锁表持续增长

## Phase 1 文档收口

- [ ] 把 `remote` 在 standalone 中仍复用本地 WASM 调用路径的事实写清楚，不再暗示已有真实的 Controller/Node 执行边界
- [ ] 把 `fan-out`、`fork`、`join`、自动 `subprocess` 编排当前未完整支持的事实写清楚，不再让文档和示例看起来像已可用能力
- [ ] 收紧 `docs/roadmap.md` 对 deploy 静态检查的承诺；当前并不能静态证明 manifest 中命名的 `action` / `guard` / `aggregator` 在 guest 内部分支里真的实现
- [ ] 收紧 `docs/event-sourcing.md` 中对事件类型和审计范围的描述；当前没有单独的“外部触发事件”记录，也没有 actor/source 信息
- [ ] 收紧 `docs/event-sourcing.md` / `docs/core-concepts.md` 中对 payload、guard 拒绝、排队事件、取消原因等可审计性的暗示
- [ ] 收紧 `docs/wasm-design.md` 中对 capability 粒度的表述；runtime 已按 `action.capabilities` 在每次调用时动态放行 network/storage
- [ ] 收紧 `docs/wasm-design.md` 中对运行时限制的表述；当前只有固定 fuel 预算，`memory_mb` / `timeout_ms` / `max_concurrent` 尚无对应配置面
- [ ] 收紧 `docs/wasm-design.md` 中 `network` world 的安全描述；当前没有 domain allowlist，guest 还可配置 proxy、local address、自定义 root CA 和危险 TLS 选项
- [ ] 收紧 `docs/wasm-design.md` 中 `storage` world 的安全描述；当前拿到 storage 权限的 guest 可以读写任意 namespace，没有 flow/job 级隔离
- [ ] 收紧 `docs/backends.md` 中 “standalone 使用 in-process transport” 的表述；当前 transport 抽象并未进入主执行链路
- [ ] 收紧 `docs/scheduling.md` 中关于 action timeout、retry/backoff、Node 心跳、负载感知、drain、背压的表述；这些能力当前未接入 runtime
- [ ] 收紧 `docs/operations.md` 中 Node 注册、发现、健康检查、优雅停机、OpenTelemetry 指标/追踪的现状表述
- [ ] 修正文档里对定时器实现的性能/结构暗示；当前实现是一计时器一 `tokio::sleep` 任务，不是 hierarchical timer wheel
- [ ] 明确 action `output` 当前会被 runtime 丢弃，不会持久化，也不会反馈给后续流程
- [ ] 明确 `CreateJobRequest.context` 当前更像持久化元数据，而不是 guest 可读的正式运行时上下文
- [ ] 明确 Flow 删除后 `wasm_modules` 与 `module_cache` 当前没有引用计数/共享生命周期策略
- [ ] 统一示例中的平台 `flow_id` 与 guest `manifest.id` 叙事；当前部分 example 在测试里被部署到不同外部 flow_id，会强化身份漂移的默认印象

## 后续阶段再做

- [ ] 真正实现 `remote` 的 Controller/Node 执行边界，而不是继续把它当 `local` 的语义标签
- [ ] 真正实现 `fan-out` 分发、结果收集与聚合后的状态推进
- [ ] 真正实现 `fork` / `join` 的运行时语义
- [ ] 真正实现 `subprocess` 的父子 Job 编排、回注和关联管理
- [ ] 如需保留 `force_delete_job` / `force_delete_flow`，下沉为服务端原子/半原子操作，而不是继续由客户端顺序拼装
- [ ] 如需保留 `job ls --all`，增加服务端 `ListJobsAll` 能力，而不是继续通过 flow 清单拼接
- [ ] 明确 `save_job_with_event` 的跨后端原子性契约；当前只有覆写该方法的后端才真正单事务提交
- [ ] 为内存中的 `flow_registry.versioned_*` / `module_cache` 建立 retention / GC 策略
- [ ] 为 `wasm_modules` 建立引用计数或共享生命周期策略，避免孤儿字节长期堆积
- [ ] 如果继续开放 `fanout_action!` / `remote_action!`，给它们明确的 stable/experimental 分层
- [ ] 收紧 `shiroha-sdk` 宏对非法 manifest shape 的放行；当前 `flow_state!(..., Subprocess)` 可在缺少 `subprocess` 配置时照样生成 manifest
- [ ] 清理 `shiroha-sdk` build script 的 staged WIT 目录；当前只会复制现有文件，不会删除已移除/重命名的旧文件
- [ ] 修正测试夹具缓存键；当前 `wasm_for_manifest()` 只按 manifest JSON 缓存，`example_wasm()` 也不会跟踪 `shiroha-sdk` 代码变化
- [ ] 强化 fixture/example 缓存哈希；当前 `compute_hash()` 只取长度、前 16 字节和后 16 字节，理论上会发生碰撞并复用错误产物
- [ ] 如果要把 `action output` 变成正式能力，设计并实现其持久化/传递语义
- [ ] 如果要把 `CreateJobRequest.context` 变成正式能力，设计并实现其进入 guest 的 ABI 和读取方式
- [ ] 如果要保留 network capability 的安全承诺，实现真正的 allowlist / policy 层
- [ ] 如果要把 storage capability 作为安全边界，实现 flow/job 级 namespace 隔离
- [ ] 如果要把事件溯源作为完整审计系统，补齐 actor/source/payload ref/guard reject/triggered-event 等事件模型
- [ ] 如需更完整的资源控制，再实现 `memory_mb` / `timeout_ms` / `max_concurrent` 等运行时限制配置

## 建议测试

- [x] 增加“同一 `(from, event)` 下多候选转移按 guard 选边”的失败用例
- [x] 增加“初始 `on-enter` 失败时 create-job 的可见语义”测试
- [x] 增加“转移已提交但 action 失败时 trigger-event 的可见语义”测试
- [x] 增加“`fan-out` flow 在 deploy 期被拒绝或被明确标记 unsupported”测试
- [x] 增加“`kind = subprocess` 但缺少 subprocess 配置”的 deploy 校验测试
- [x] 增加“旧版本后写入时 latest alias 不能回退”的存储测试
- [x] 增加“同毫秒多事件时 `GetJobEvents.since_id` 仍稳定”的查询测试
- [ ] 增加“`GetJobEvents.limit` 与后端无关”的测试
- [ ] 增加“删除 Flow 后 wasm bytes / module cache 生命周期符合约定”的测试
