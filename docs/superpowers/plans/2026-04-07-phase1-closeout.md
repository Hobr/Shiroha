# Phase 1 Closeout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close Phase 1 by making the documented MVP contract match the actual runtime model and by landing the last small user-facing lifecycle control, `max_lifetime`, without widening the execution model beyond the current standalone design.

**Architecture:** Treat Phase 1 closeout as a contract-convergence pass, not a mini-rewrite. Update docs to describe the runtime that actually exists today (job-level serialization, paused-event queue, restart recovery of job snapshots/timeouts only), then add `max_lifetime` end-to-end as a bounded feature using the existing timer wheel and job cancellation path. Do not attempt to implement a fully generalized persisted event inbox or in-flight action recovery in this phase.

**Tech Stack:** Rust 1.94.1, tonic/protobuf, Tokio timer wheel, redb, clap, cargo nextest

---

## Planned File Structure

**Modify**

- `docs/core-concepts.md`
- `docs/scheduling.md`
- `docs/event-sourcing.md`
- `docs/roadmap.md`
- `crate/shiroha-proto/proto/shiroha.proto`
- `crate/shiroha-core/src/job.rs`
- `crate/shiroha-engine/src/job.rs`
- `app/shirohad/src/job_service.rs`
- `app/shirohad/src/server.rs`
- `crate/shiroha-client/src/job.rs`
- `app/sctl/src/client.rs`
- `app/sctl/src/cli_support.rs`
- `app/sctl/src/main.rs`
- `app/sctl/src/command_runner.rs`

### Task 1: Re-Baseline the Phase 1 Docs to the Actual Runtime Contract

**Files:**
- Modify: `docs/core-concepts.md`
- Modify: `docs/scheduling.md`
- Modify: `docs/event-sourcing.md`
- Modify: `docs/roadmap.md`

- [ ] **Step 1: Update `core-concepts.md` to describe the current Job execution model honestly**

Replace the “event inbox” wording with the current contract:

```md
### Job 并发控制

当前 Phase 1 的正确性保证来自 **Job 级串行锁 + 暂停期间事件队列**：

- 同一个 Job 在 Controller 内任一时刻只有一个事件处理临界区
- `running` 状态下，外部事件会在持锁后立即处理
- `paused` 状态下，事件会持久化到 Job 快照里的队列，`resume` 后按顺序回放
- 当前还没有一个“所有状态通用、持久化的 FIFO event inbox” 抽象
```

- [ ] **Step 2: Update `scheduling.md` to narrow the restart guarantee**

Change the recovery wording from “恢复包括 in-flight 任务” to the exact currently supported scope:

```md
- **Controller 恢复**：重启后恢复持久化的 Job 快照、暂停期间事件队列、timeout 计划、Flow 版本和 WASM 模块缓存
- 当前不恢复运行中的 in-flight Action；重启后的继续执行以持久化快照为边界
```

- [ ] **Step 3: Update `event-sourcing.md` to match the real recovery boundary**

Use wording like:

```md
- **故障恢复**：Controller 重启后恢复持久化的 Job 快照、Flow 版本、WASM 模块、暂停事件队列和 timeout 计划
- 当前不会从事件日志或宿主句柄层面恢复 in-flight Action 执行
```

- [ ] **Step 4: Update `roadmap.md` to move the two larger overclaims out of Phase 1**

Keep Phase 1 scoped to the current runtime and move the larger work to a later phase:

```md
### Phase 1 — 单机可用（MVP）
- Job 并发控制：每个 Job 串行化事件处理；paused 状态下事件持久化排队
- 重启恢复：重新加载 Flow 版本和模块缓存，恢复 Job 快照、暂停事件队列和 timeout 计划

### Phase 2 / 3
- 通用持久化 event inbox
- in-flight Action 跟踪 / 取消 / 恢复
```

- [ ] **Step 5: Commit the doc re-baseline**

```bash
git add docs/core-concepts.md docs/scheduling.md docs/event-sourcing.md docs/roadmap.md
git commit -m "docs: align phase1 contract with runtime semantics"
```

### Task 2: Add `max_lifetime_ms` End-to-End

**Files:**
- Modify: `crate/shiroha-proto/proto/shiroha.proto`
- Modify: `crate/shiroha-core/src/job.rs`
- Modify: `crate/shiroha-engine/src/job.rs`
- Modify: `app/shirohad/src/job_service.rs`
- Modify: `app/shirohad/src/server.rs`
- Modify: `crate/shiroha-client/src/job.rs`
- Modify: `app/sctl/src/client.rs`
- Modify: `app/sctl/src/cli_support.rs`
- Modify: `app/sctl/src/main.rs`
- Modify: `app/sctl/src/command_runner.rs`
- Test: `app/shirohad/src/job_service.rs`
- Test: `app/shirohad/src/server.rs`

- [ ] **Step 1: Write the failing job-lifetime tests**

Add a service-level regression in `app/shirohad/src/job_service.rs`:

```rust
#[tokio::test]
async fn create_job_with_max_lifetime_auto_cancels_job() {
    let harness = TestHarness::with_timer_forwarder("job-max-lifetime").await;
    deploy_flow(
        harness.state.clone(),
        "lifetime-flow",
        &approval_manifest("lifetime-flow", Some("allow")),
    )
    .await;
    let service = JobServiceImpl::new(harness.state.clone());

    let created = service
        .create_job(Request::new(CreateJobRequest {
            flow_id: "lifetime-flow".into(),
            context: None,
            max_lifetime_ms: Some(50),
        }))
        .await
        .expect("create job")
        .into_inner();

    let job = wait_for_job(&service, &created.job_id, "cancelled", "idle").await;
    assert_eq!(job.state, "cancelled");
}
```

Add a restart regression in `app/shirohad/src/server.rs`:

```rust
#[tokio::test]
async fn reloaded_server_preserves_job_lifetime_deadline() {
    let harness = TestHarness::new("server-reload-lifetime").await;
    let data_dir = harness.data_dir.clone();
    deploy_flow(
        harness.state.clone(),
        "lifetime-flow",
        &approval_manifest("lifetime-flow", Some("allow")),
    )
    .await;

    let job_service = JobServiceImpl::new(harness.state.clone());
    let created = job_service
        .create_job(Request::new(CreateJobRequest {
            flow_id: "lifetime-flow".into(),
            context: None,
            max_lifetime_ms: Some(200),
        }))
        .await
        .expect("create job")
        .into_inner();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    drop(job_service);
    drop(harness);

    let reloaded = ShirohaServer::new(data_dir.to_str().expect("utf-8 path"))
        .await
        .expect("reload server");
    let job_service = JobServiceImpl::new(reloaded.state.clone());

    let job = wait_for_job(&job_service, &created.job_id, "cancelled", "idle").await;
    assert_eq!(job.state, "cancelled");
}
```

- [ ] **Step 2: Run the RED check**

Run: `cargo test -p shirohad job_service::tests::create_job_with_max_lifetime_auto_cancels_job -- --exact`
Expected: FAIL because `CreateJobRequest` and runtime do not support `max_lifetime_ms` yet.

- [ ] **Step 3: Add the API and snapshot fields**

Update protobuf and `Job` snapshot shape:

```proto
message CreateJobRequest {
  string flow_id = 1;
  optional bytes context = 2;
  optional uint64 max_lifetime_ms = 3;
}
```

```rust
pub struct Job {
    pub id: Uuid,
    pub flow_id: String,
    pub flow_version: Uuid,
    pub state: JobState,
    pub current_state: String,
    pub context: Option<Vec<u8>>,
    pub pending_events: Vec<PendingJobEvent>,
    pub scheduled_timeouts: Vec<ScheduledTimeout>,
    pub timeout_anchor_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_lifetime_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifetime_deadline_ms: Option<u64>,
}
```

- [ ] **Step 4: Implement wall-clock lifetime cancellation on top of the existing timer wheel**

Use a reserved internal event name and keep the logic narrow:

```rust
const JOB_LIFETIME_EXPIRED_EVENT: &str = "__shiroha.job_lifetime_expired__";
```

```rust
// create_job path
let deadline = req.max_lifetime_ms.map(|ms| now_ms() + ms);
let job = self
    .state
    .job_manager
    .create_job(
        &flow.flow_id,
        flow.version,
        &flow.manifest.initial_state,
        req.context,
        req.max_lifetime_ms,
        deadline,
    )
    .await?;

if let Some(deadline) = deadline {
    let remaining = deadline.saturating_sub(now_ms());
    self.state
        .timer_wheel
        .register(
            job.id,
            JOB_LIFETIME_EXPIRED_EVENT.to_string(),
            std::time::Duration::from_millis(remaining),
        )
        .await;
}
```

```rust
// enqueue path
if event == JOB_LIFETIME_EXPIRED_EVENT {
    self.state
        .job_manager
        .cancel_job(job_id)
        .await
        .map_err(|e| Status::failed_precondition(e.to_string()))?;
    self.state.timer_wheel.cancel_all_job_timers(job_id).await;
    return Ok(());
}
```

```rust
// restart recovery
if let Some(deadline) = job.lifetime_deadline_ms {
    let remaining = deadline.saturating_sub(now_ms);
    state
        .timer_wheel
        .register(
            job.id,
            JOB_LIFETIME_EXPIRED_EVENT.to_string(),
            std::time::Duration::from_millis(remaining),
        )
        .await;
}
```

- [ ] **Step 5: Plumb `max_lifetime_ms` through client and CLI**

Add CLI flag and forward it:

```rust
#[derive(Args)]
struct JobCreateArgs {
    #[command(flatten)]
    flow: FlowIdArgs,
    #[command(flatten)]
    context: ContextArgs,
    #[arg(long, value_name = "MS", value_parser = parse_positive_u64)]
    max_lifetime_ms: Option<u64>,
}
```

```rust
pub(crate) fn parse_positive_u64(input: &str) -> Result<u64, String> {
    parse_positive(input)
}
```

```rust
pub async fn create_job(
    &mut self,
    flow_id: &str,
    context: Option<Vec<u8>>,
    max_lifetime_ms: Option<u64>,
) -> anyhow::Result<CreateJobResponse> {
    Ok(self
        .job
        .create_job(CreateJobRequest {
            flow_id: flow_id.to_string(),
            context,
            max_lifetime_ms,
        })
        .await?
        .into_inner())
}
```

- [ ] **Step 6: Run the targeted lifetime verification**

Run: `cargo nextest run -p shirohad create_job_with_max_lifetime_auto_cancels_job reloaded_server_preserves_job_lifetime_deadline`
Expected: PASS.

- [ ] **Step 7: Commit the feature**

```bash
git add crate/shiroha-proto/proto/shiroha.proto crate/shiroha-core/src/job.rs crate/shiroha-engine/src/job.rs app/shirohad/src/job_service.rs app/shirohad/src/server.rs crate/shiroha-client/src/job.rs app/sctl/src/client.rs app/sctl/src/cli_support.rs app/sctl/src/main.rs app/sctl/src/command_runner.rs
git commit -m "feat: add job max lifetime for phase1"
```

### Task 3: Lock the Final Phase 1 Contract with Acceptance Tests and Final Docs

**Files:**
- Modify: `docs/core-concepts.md`
- Modify: `docs/roadmap.md`
- Modify: `app/shirohad/src/job_service.rs`
- Modify: `app/shirohad/src/server.rs`

- [ ] **Step 1: Add or refresh acceptance tests for the final supported guarantees**

Keep the closeout contract explicit:

```rust
#[tokio::test]
async fn paused_job_persists_event_queue_until_resume() {
    // pause -> trigger event -> restart optional -> resume -> event drains in order
}

#[tokio::test]
async fn restart_restores_timeouts_and_lifetime_but_not_running_action_state() {
    // assert snapshot/timer-based recovery boundary, not in-flight action replay
}
```

- [ ] **Step 2: Update docs to the final closed Phase 1 wording**

After Task 2 lands, `max_lifetime` can remain in `core-concepts.md`, but the other two items stay narrowed:

```md
- Job 级别可配置 `max_lifetime`，超时后由 Controller 自动取消
- 当前 Phase 1 不提供通用持久化 event inbox；暂停期间事件会持久化排队
- 当前重启恢复不恢复 in-flight Action 执行
```

- [ ] **Step 3: Run the closeout subset**

Run: `cargo nextest run -p shirohad paused_job_persists_event_queue_until_resume reloaded_server_preserves_paused_job_pending_events reloaded_server_restores_running_job_timers create_job_with_max_lifetime_auto_cancels_job reloaded_server_preserves_job_lifetime_deadline`
Expected: PASS.

- [ ] **Step 4: Commit the closeout contract**

```bash
git add docs/core-concepts.md docs/roadmap.md app/shirohad/src/job_service.rs app/shirohad/src/server.rs
git commit -m "test: lock phase1 closeout contract"
```

### Task 4: Final Verification

**Files:**
- Modify: files changed in Tasks 1-3

- [ ] **Step 1: Run workspace compile verification**

```bash
cargo check --workspace
```

Expected: exit code 0.

- [ ] **Step 2: Run strict lint verification**

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: exit code 0.

- [ ] **Step 3: Run full test verification**

```bash
cargo nextest run --all-features --no-tests=warn
```

Expected: all non-ignored tests PASS.

- [ ] **Step 4: Run formatting and pre-commit verification**

```bash
just fmt
```

Expected: exit code 0.

- [ ] **Step 5: Commit the Phase 1 closeout plan artifact**

```bash
git add docs/superpowers/plans/2026-04-07-phase1-closeout.md
git commit -m "docs: add phase1 closeout plan"
```
