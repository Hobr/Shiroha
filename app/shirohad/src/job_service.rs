//! gRPC JobService 实现
//!
//! 处理 Job 的创建、状态查询、事件触发、生命周期管理（暂停/恢复/取消）。
//! Phase 1 里所有事件都按 Job 串行处理，暂停期间事件会跟随 Job 快照一起持久化。

use std::collections::VecDeque;
use std::sync::Arc;

use shiroha_core::error::ShirohaError;
use shiroha_core::flow::{
    ActionCapability, DispatchMode, FanOutConfig, FanOutStrategy, FlowRegistration, StateKind,
    TimeoutDef,
};
use shiroha_core::job::{
    ActionResult, AggregateDecision, ExecutionStatus, Job, JobState, NodeResult, PendingJobEvent,
    ScheduledTimeout,
};
use shiroha_core::transport::{Message, Transport};
use shiroha_engine::job::JobCreationOptions;
use shiroha_proto::shiroha_api::job_service_server::JobService;
use shiroha_proto::shiroha_api::{
    CancelJobRequest, CancelJobResponse, CreateJobRequest, CreateJobResponse, DeleteJobRequest,
    DeleteJobResponse, GetJobEventsRequest, GetJobEventsResponse, GetJobRequest, GetJobResponse,
    ListAllJobsRequest, ListJobsRequest, ListJobsResponse, PauseJobRequest, PauseJobResponse,
    ResumeJobRequest, ResumeJobResponse, TriggerEventRequest, TriggerEventResponse,
};
use shiroha_wasm::host::{ActionContext, GuardContext, WasmHost};
use tokio::task::JoinSet;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::job_events::{filter_events, validate_query};
use crate::job_runtime::action_sequence;
use crate::node_runtime::{RemoteActionRequest, RemoteActionResponse, STANDALONE_NODE_ID};
use crate::server::ShirohaState;
use crate::service_support::parse_uuid;

pub(crate) const JOB_LIFETIME_EXPIRED_EVENT: &str = "__shiroha.job_lifetime_expired__";

#[derive(Debug, Clone)]
struct TransitionCandidatePlan {
    from: String,
    to: String,
    action: Option<String>,
    guard: Option<String>,
    on_exit: Option<String>,
    on_enter: Option<String>,
    is_terminal: bool,
    timeouts: Vec<TimeoutDef>,
}

struct DeclaredActionOutcome {
    warning: Option<String>,
    follow_up: Option<PendingJobEvent>,
}

struct FanoutSlotOutcome {
    node_result: NodeResult,
}

/// JobService 的 standalone 实现。
///
/// 它把 gRPC 请求、定时器回调和 WASM 调用串成一条统一的 Job 处理链路。
pub struct JobServiceImpl {
    state: Arc<ShirohaState>,
}

impl JobServiceImpl {
    fn fanout_slots(config: &FanOutConfig) -> Vec<String> {
        match &config.strategy {
            FanOutStrategy::All => vec![STANDALONE_NODE_ID.to_string()],
            FanOutStrategy::Count(count) => (0..*count)
                .map(|index| format!("{STANDALONE_NODE_ID}-{}", index + 1))
                .collect(),
            FanOutStrategy::Tagged(tags) => tags.clone(),
        }
    }

    pub fn new(state: Arc<ShirohaState>) -> Self {
        Self { state }
    }

    fn job_response(job: &Job) -> GetJobResponse {
        GetJobResponse {
            job_id: job.id.to_string(),
            flow_id: job.flow_id.clone(),
            state: job.state.to_string(),
            current_state: job.current_state.clone(),
            flow_version: job.flow_version.to_string(),
            context_bytes: job.context.as_ref().map(|context| context.len() as u64),
            max_lifetime_ms: job.max_lifetime_ms,
            lifetime_deadline_ms: job.lifetime_deadline_ms,
            remaining_lifetime_ms: Self::remaining_lifetime_ms(job),
        }
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn remaining_lifetime_ms(job: &Job) -> Option<u64> {
        job.lifetime_deadline_ms
            .map(|deadline| deadline.saturating_sub(Self::now_ms()))
    }

    async fn register_lifetime_timer_if_needed(&self, job: &Job) {
        if !matches!(job.state, JobState::Running | JobState::Paused) {
            return;
        }
        if let Some(remaining) = Self::remaining_lifetime_ms(job) {
            self.state
                .timer_wheel
                .register_with_policy(
                    job.id,
                    JOB_LIFETIME_EXPIRED_EVENT.to_string(),
                    std::time::Duration::from_millis(remaining),
                    false,
                )
                .await;
        }
    }

    pub async fn enqueue_event(
        &self,
        job_id: Uuid,
        event: String,
        payload: Option<Vec<u8>>,
    ) -> Result<(), Status> {
        // 所有入口都先拿到同一把 Job 锁，确保单个 Job 的状态转移严格串行。
        let lock = self.job_lock(job_id).await;
        let _guard = lock.lock().await;
        let result = self.handle_event_locked(job_id, event, payload).await;
        if result
            .as_ref()
            .is_err_and(|error| error.code() == tonic::Code::NotFound)
        {
            self.remove_job_lock(job_id).await;
        }
        result
    }

    async fn job_lock(&self, job_id: Uuid) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.state.job_locks.lock().await;
        locks
            // 锁按需创建并常驻，后续同一个 job_id 可以复用同一条串行化通道。
            .entry(job_id)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    async fn load_job(&self, job_id: Uuid) -> Result<Job, Status> {
        self.state
            .job_manager
            .get_job(job_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("job not found"))
    }

    async fn flow_registration(&self, flow_id: &str) -> Result<FlowRegistration, Status> {
        self.state
            .flow_registry
            .latest_registration(flow_id)
            .await
            .ok_or_else(|| Status::internal(format!("flow `{flow_id}` not loaded in memory")))
    }

    async fn flow_registration_for_job(&self, job: &Job) -> Result<FlowRegistration, Status> {
        self.state
            .flow_registry
            .versioned_registration(&job.flow_id, job.flow_version)
            .await
            .ok_or_else(|| {
                Status::internal(format!(
                    "flow `{}` version {} not loaded in memory",
                    job.flow_id, job.flow_version
                ))
            })
    }

    async fn remove_job_lock(&self, job_id: Uuid) {
        self.state.job_locks.lock().await.remove(&job_id);
    }

    async fn handle_event_locked(
        &self,
        job_id: Uuid,
        event: String,
        payload: Option<Vec<u8>>,
    ) -> Result<(), Status> {
        let mut events = VecDeque::from([PendingJobEvent { event, payload }]);
        self.process_event_sequence_locked(job_id, &mut events)
            .await
    }

    async fn process_event_sequence_locked(
        &self,
        job_id: Uuid,
        events: &mut VecDeque<PendingJobEvent>,
    ) -> Result<(), Status> {
        while let Some(PendingJobEvent { event, payload }) = events.pop_front() {
            let job = self.load_job(job_id).await?;
            if event == JOB_LIFETIME_EXPIRED_EVENT {
                match job.state {
                    JobState::Running | JobState::Paused => {
                        self.state
                            .job_manager
                            .cancel_job(job_id)
                            .await
                            .map_err(|e| Status::failed_precondition(e.to_string()))?;
                        self.state.timer_wheel.cancel_all_job_timers(job_id).await;
                        self.remove_job_lock(job_id).await;
                        tracing::info!(job_id = %job_id, "job lifetime expired; cancelled job");
                        return Ok(());
                    }
                    JobState::Cancelled | JobState::Completed => return Ok(()),
                }
            }

            match job.state {
                JobState::Running => {
                    let mut follow_ups = self.process_running_event(job, event, payload).await?;
                    while let Some(follow_up) = follow_ups.pop() {
                        events.push_front(follow_up);
                    }
                }
                JobState::Paused => {
                    self.state
                        .job_manager
                        .queue_pending_event(job_id, event.clone(), payload)
                        .await
                        .map_err(|e| Status::internal(e.to_string()))?;
                    while let Some(queued) = events.pop_back() {
                        self.state
                            .job_manager
                            .push_front_pending_event(job_id, queued)
                            .await
                            .map_err(|e| Status::internal(e.to_string()))?;
                    }
                    tracing::info!(job_id = %job_id, event, "job paused; queued event");
                    return Ok(());
                }
                JobState::Cancelled | JobState::Completed => {
                    return Err(Status::failed_precondition(format!("job is {}", job.state)));
                }
            }
        }

        Ok(())
    }

    async fn process_running_event(
        &self,
        job: Job,
        event: String,
        payload: Option<Vec<u8>>,
    ) -> Result<Vec<PendingJobEvent>, Status> {
        let flow = self.flow_registration_for_job(&job).await?;
        // 只在 engine 访问阶段完成“查拓扑、拿候选边”这类纯读操作，
        // guard 选择和后续 WASM 调用都在锁外完成，避免阻塞其他 Job 读取同一个 Flow。
        let candidates = {
            let engine = self
                .state
                .flow_registry
                .versioned_engine(&job.flow_id, job.flow_version)
                .await
                .ok_or_else(|| Status::internal("engine not found for flow"))?;
            let from_state = engine
                .get_state(&job.current_state)
                .ok_or_else(|| Status::internal("source state not found in manifest"))?;
            let transitions = engine.find_transitions(&job.current_state, &event);
            if transitions.is_empty() {
                return Err(Status::failed_precondition(
                    ShirohaError::InvalidTransition {
                        from: job.current_state.clone(),
                        to: String::new(),
                        event: event.clone(),
                    }
                    .to_string(),
                ));
            }

            transitions
                .into_iter()
                .map(|transition| {
                    let to_state = engine
                        .get_state(&transition.to)
                        .ok_or_else(|| Status::internal("target state not found in manifest"))?;
                    let next_timeouts = if engine.is_terminal(&transition.to) {
                        Vec::new()
                    } else {
                        engine
                            .manifest()
                            .transitions
                            .iter()
                            .filter(|candidate| candidate.from == transition.to)
                            .filter_map(|candidate| candidate.timeout.clone())
                            .collect()
                    };

                    Ok(TransitionCandidatePlan {
                        from: transition.from.clone(),
                        to: transition.to.clone(),
                        action: transition.action.clone(),
                        guard: transition.guard.clone(),
                        on_exit: from_state.on_exit.clone(),
                        on_enter: to_state.on_enter.clone(),
                        is_terminal: engine.is_terminal(&transition.to),
                        timeouts: next_timeouts,
                    })
                })
                .collect::<Result<Vec<_>, Status>>()?
        };

        let mut selected = None;
        for candidate in candidates {
            if let Some(guard_name) = candidate.guard.as_deref() {
                let allowed = self
                    .invoke_guard(
                        &flow,
                        guard_name,
                        GuardContext {
                            job_id: job.id.to_string(),
                            from_state: candidate.from.clone(),
                            to_state: candidate.to.clone(),
                            event: event.clone(),
                            context: job.context.clone(),
                            payload: payload.clone(),
                        },
                    )
                    .await?;
                if !allowed {
                    continue;
                }
            }

            selected = Some(candidate);
            break;
        }

        let Some(TransitionCandidatePlan {
            from,
            to,
            action,
            on_exit,
            on_enter,
            is_terminal,
            timeouts,
            ..
        }) = selected
        else {
            return Err(Status::failed_precondition(
                ShirohaError::GuardRejected.to_string(),
            ));
        };

        // 一旦离开旧状态，旧状态上的 timeout 全部失效，因此先整体撤销再按新状态重建。
        self.state.timer_wheel.cancel_all_job_timers(job.id).await;
        self.state
            .job_manager
            .transition_job_with_schedule(
                job.id,
                &event,
                &from,
                &to,
                action.clone(),
                if is_terminal {
                    Vec::new()
                } else {
                    timeouts
                        .iter()
                        .map(|timeout| ScheduledTimeout {
                            event: timeout.timeout_event.clone(),
                            remaining_ms: timeout.duration_ms,
                        })
                        .collect()
                },
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let mut action_failures = Vec::new();
        let mut follow_ups = Vec::new();
        for scheduled in action_sequence(
            from.as_str(),
            to.as_str(),
            on_exit.as_deref(),
            action.as_deref(),
            on_enter.as_deref(),
        ) {
            let outcome = self
                .run_declared_action(
                    &flow,
                    scheduled.action_name(),
                    job.id,
                    scheduled.action_state(),
                    job.context.clone(),
                    payload.clone(),
                )
                .await?;
            if let Some(message) = outcome.warning {
                action_failures.push(message);
            }
            if let Some(follow_up) = outcome.follow_up {
                follow_ups.push(follow_up);
            }
        }

        if is_terminal {
            // 终态不再保留任何待触发 timeout。
            self.state
                .job_manager
                .complete_job(job.id, &to)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
            self.state.timer_wheel.cancel_all_job_timers(job.id).await;
            self.remove_job_lock(job.id).await;
        } else {
            // timeout 是“状态出边”的属性，所以进入新状态后要重新扫描所有出边注册。
            self.rearm_persisted_timeouts(job.id).await?;
            self.register_lifetime_timer_if_needed(&job).await;
        }

        tracing::info!(job_id = %job.id, event, from, to, "event processed");

        if !action_failures.is_empty() {
            tracing::warn!(
                job_id = %job.id,
                failures = ?action_failures,
                "event committed with action failures"
            );
        }

        Ok(if is_terminal { Vec::new() } else { follow_ups })
    }

    async fn drain_pending_events_locked(&self, job_id: Uuid) -> Result<(), Status> {
        let mut events = VecDeque::new();
        while let Some(queued) = self
            .state
            .job_manager
            .pop_pending_event(job_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
        {
            events.push_back(queued);
        }

        self.process_event_sequence_locked(job_id, &mut events)
            .await
    }

    async fn record_action_result(
        &self,
        job_id: Uuid,
        action: &str,
        result: &ActionResult,
    ) -> Result<(), Status> {
        self.state
            .job_manager
            .record_action_result(job_id, action, Some("standalone".into()), result.status)
            .await
            .map_err(|e| Status::internal(e.to_string()))
    }

    async fn invoke_aggregate(
        &self,
        flow: &FlowRegistration,
        name: &str,
        results: Vec<NodeResult>,
    ) -> Result<AggregateDecision, Status> {
        let state = self.state.clone();
        let flow = flow.clone();
        let name = name.to_string();
        tokio::task::spawn_blocking(move || {
            let mut host = Self::wasm_host_for_state(&state, &flow)?;
            host.aggregate(&name, &results)
                .map_err(|e| Status::internal(e.to_string()))
        })
        .await
        .map_err(|error| Status::internal(format!("aggregate task join error: {error}")))?
    }

    async fn run_fanout_action(
        &self,
        flow: &FlowRegistration,
        action_name: &str,
        action_ctx: ActionContext,
        config: &FanOutConfig,
    ) -> Result<DeclaredActionOutcome, Status> {
        let slots = Self::fanout_slots(config);
        let deadline = config.timeout_ms.map(|timeout_ms| {
            tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms)
        });
        let job_id: Uuid = action_ctx
            .job_id
            .parse()
            .map_err(|_| Status::internal("fan-out job id should stay parseable"))?;
        let capabilities = flow
            .manifest
            .actions
            .iter()
            .find(|candidate| candidate.name == action_name)
            .map(|action| action.capabilities.clone())
            .unwrap_or_default();

        let mut join_set = JoinSet::new();
        for slot in slots {
            let request = RemoteActionRequest {
                flow_id: flow.flow_id.clone(),
                flow_version: flow.version,
                action_name: action_name.to_string(),
                action_ctx: action_ctx.clone(),
                capabilities: capabilities.clone(),
            };
            let transport = self.state.transport.clone();
            join_set.spawn(async move {
                let payload = serde_json::to_vec(&request)
                    .map_err(|error| format!("encode fan-out action request: {error}"))?;
                let response = transport
                    .send(STANDALONE_NODE_ID, Message { payload })
                    .await
                    .map_err(|error| error.to_string())?;

                let decoded: RemoteActionResponse = serde_json::from_slice(&response.payload)
                    .map_err(|error| format!("decode fan-out action response: {error}"))?;
                let node_result = match (decoded.result, decoded.error) {
                    (Some(result), None) => NodeResult {
                        node_id: slot,
                        status: result.status,
                        output: result.output,
                    },
                    (None, Some(error)) => NodeResult {
                        node_id: slot,
                        status: ExecutionStatus::Failed,
                        output: Some(error.into_bytes()),
                    },
                    _ => {
                        return Err(
                            "fan-out action response must contain exactly one of result or error"
                                .to_string(),
                        );
                    }
                };

                Ok(FanoutSlotOutcome { node_result })
            });
        }

        let mut success_count = 0u32;
        let mut aggregate_results = Vec::new();
        let mut cutoff_reached = false;
        loop {
            let maybe_joined = if let Some(deadline) = deadline {
                tokio::select! {
                    joined = join_set.join_next(), if !join_set.is_empty() => joined,
                    _ = tokio::time::sleep_until(deadline), if !join_set.is_empty() => {
                        cutoff_reached = true;
                        continue;
                    }
                    else => None,
                }
            } else {
                join_set.join_next().await
            };

            let Some(joined) = maybe_joined else {
                break;
            };
            let outcome = joined
                .map_err(|error| Status::internal(format!("fan-out task join error: {error}")))?
                .map_err(Status::internal)?;
            let node_result = outcome.node_result;

            self.state
                .job_manager
                .record_action_result(
                    job_id,
                    action_name,
                    Some(node_result.node_id.clone()),
                    node_result.status,
                )
                .await
                .map_err(|e| Status::internal(e.to_string()))?;

            if node_result.status == ExecutionStatus::Success {
                success_count += 1;
            }
            if !cutoff_reached {
                if let Some(deadline) = deadline
                    && tokio::time::Instant::now() >= deadline
                {
                    cutoff_reached = true;
                }
                if !cutoff_reached {
                    aggregate_results.push(node_result.clone());
                }
            }

            if config
                .min_success
                .is_some_and(|min_success| success_count >= min_success)
            {
                cutoff_reached = true;
            }
        }

        let decision = self
            .invoke_aggregate(flow, &config.aggregator, aggregate_results)
            .await?;

        Ok(DeclaredActionOutcome {
            warning: None,
            follow_up: Some(PendingJobEvent {
                event: decision.event,
                payload: decision.context_patch,
            }),
        })
    }

    async fn run_declared_action(
        &self,
        flow: &FlowRegistration,
        action_name: &str,
        job_id: Uuid,
        state: &str,
        context: Option<Vec<u8>>,
        payload: Option<Vec<u8>>,
    ) -> Result<DeclaredActionOutcome, Status> {
        let action = flow
            .manifest
            .actions
            .iter()
            .find(|candidate| candidate.name == action_name)
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "action `{action_name}` not declared in manifest"
                ))
            })?;
        let action_ctx = ActionContext {
            job_id: job_id.to_string(),
            state: state.to_string(),
            context,
            payload,
        };

        if let DispatchMode::FanOut(config) = &action.dispatch {
            return self
                .run_fanout_action(flow, action_name, action_ctx, config)
                .await;
        }

        // 当前语义里 action / state hook 都属于“状态推进后的副作用”：
        // 即使执行状态失败，转移和事件日志也已经提交，便于后续审计/补偿。
        let action_result = match self
            .invoke_action(
                flow,
                action_name,
                job_id,
                state,
                action_ctx.context,
                action_ctx.payload,
            )
            .await
        {
            Ok(result) => result,
            Err(error) => {
                let synthetic = ActionResult {
                    status: ExecutionStatus::Failed,
                    output: None,
                };
                if let Err(record_error) = self
                    .record_action_result(job_id, action_name, &synthetic)
                    .await
                {
                    tracing::error!(
                        job_id = %job_id,
                        action = action_name,
                        error = %record_error,
                        "failed to persist synthetic action result after committed transition"
                    );
                }
                return Ok(DeclaredActionOutcome {
                    warning: Some(format!(
                        "transition committed but action `{action_name}` failed to execute: {}",
                        error.message()
                    )),
                    follow_up: None,
                });
            }
        };
        if let Err(record_error) = self
            .record_action_result(job_id, action_name, &action_result)
            .await
        {
            tracing::error!(
                job_id = %job_id,
                action = action_name,
                error = %record_error,
                "failed to persist action result after committed transition"
            );
        }
        if action_result.status == ExecutionStatus::Success {
            return Ok(DeclaredActionOutcome {
                warning: None,
                follow_up: None,
            });
        }

        Ok(DeclaredActionOutcome {
            warning: Some(format!(
                "transition committed but action `{action_name}` finished with status {}",
                execution_status_name(action_result.status)
            )),
            follow_up: None,
        })
    }

    async fn rearm_persisted_timeouts(&self, job_id: Uuid) -> Result<(), Status> {
        let job = self.load_job(job_id).await?;
        for timeout in job.scheduled_timeouts {
            self.state
                .timer_wheel
                .register(
                    job_id,
                    timeout.event,
                    std::time::Duration::from_millis(timeout.remaining_ms),
                )
                .await;
        }
        Ok(())
    }

    fn wasm_host_for_state(
        state: &Arc<ShirohaState>,
        flow: &FlowRegistration,
    ) -> Result<WasmHost, Status> {
        let module = state
            .module_cache
            .get(&flow.wasm_hash)
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "WASM module for flow `{}` is not available in cache; redeploy the flow in this process",
                    flow.flow_id
                ))
            })?;

        // 缓存中保存的是编译后的 component；每次调用仍会创建新的 guest 实例。
        WasmHost::new_with_capability_store(
            state.wasm_runtime.engine(),
            module.component(),
            state.storage.clone(),
        )
        .map_err(|e| Status::internal(e.to_string()))
    }

    async fn invoke_guard(
        &self,
        flow: &FlowRegistration,
        guard_name: &str,
        ctx: GuardContext,
    ) -> Result<bool, Status> {
        let state = self.state.clone();
        let flow = flow.clone();
        let guard_name = guard_name.to_string();
        tokio::task::spawn_blocking(move || {
            let mut host = Self::wasm_host_for_state(&state, &flow)?;
            host.invoke_guard(&guard_name, ctx)
                .map_err(|e| Status::internal(e.to_string()))
        })
        .await
        .map_err(|error| Status::internal(format!("guard task join error: {error}")))?
    }

    async fn invoke_action(
        &self,
        flow: &FlowRegistration,
        action_name: &str,
        job_id: Uuid,
        state: &str,
        context: Option<Vec<u8>>,
        payload: Option<Vec<u8>>,
    ) -> Result<ActionResult, Status> {
        let action = flow
            .manifest
            .actions
            .iter()
            .find(|candidate| candidate.name == action_name)
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "action `{action_name}` not declared in manifest"
                ))
            })?;
        let dispatch = &action.dispatch;
        let capabilities: &[ActionCapability] = &action.capabilities;
        let action_ctx = ActionContext {
            job_id: job_id.to_string(),
            state: state.to_string(),
            context,
            payload,
        };

        match dispatch {
            DispatchMode::Local => {
                let state_handle = self.state.clone();
                let flow = flow.clone();
                let action_name = action_name.to_string();
                let capabilities = capabilities.to_vec();
                tokio::task::spawn_blocking(move || {
                    let mut host = Self::wasm_host_for_state(&state_handle, &flow)?;
                    host.invoke_action(&action_name, action_ctx, &capabilities)
                        .map_err(|e| Status::internal(e.to_string()))
                })
                .await
                .map_err(|error| Status::internal(format!("action task join error: {error}")))?
            }
            DispatchMode::Remote => {
                let request = RemoteActionRequest {
                    flow_id: flow.flow_id.clone(),
                    flow_version: flow.version,
                    action_name: action_name.to_string(),
                    action_ctx,
                    capabilities: capabilities.to_vec(),
                };
                let payload = serde_json::to_vec(&request).map_err(|error| {
                    Status::internal(format!("encode remote action request: {error}"))
                })?;
                let response = self
                    .state
                    .transport
                    .send(STANDALONE_NODE_ID, Message { payload })
                    .await
                    .map_err(|error| Status::internal(error.to_string()))?;
                let decoded: RemoteActionResponse = serde_json::from_slice(&response.payload)
                    .map_err(|error| {
                        Status::internal(format!("decode remote action response: {error}"))
                    })?;
                match (decoded.result, decoded.error) {
                    (Some(result), None) => Ok(result),
                    (None, Some(error)) => Err(Status::internal(error)),
                    _ => Err(Status::internal(
                        "remote action response must contain exactly one of result or error",
                    )),
                }
            }
            DispatchMode::FanOut(_) => Err(Status::unimplemented(
                "fan-out action dispatch is not implemented in standalone mode yet",
            )),
        }
    }
}

fn execution_status_name(status: ExecutionStatus) -> &'static str {
    match status {
        ExecutionStatus::Success => "success",
        ExecutionStatus::Failed => "failed",
        ExecutionStatus::Timeout => "timeout",
    }
}

fn map_delete_job_error(error: ShirohaError) -> Status {
    match error {
        ShirohaError::JobNotFound(_) => Status::not_found(error.to_string()),
        ShirohaError::InvalidJobState { .. } => Status::failed_precondition(error.to_string()),
        _ => Status::internal(error.to_string()),
    }
}

fn map_job_lifecycle_error(error: ShirohaError) -> Status {
    match error {
        ShirohaError::JobNotFound(_) => Status::not_found(error.to_string()),
        ShirohaError::InvalidJobState { .. } => Status::failed_precondition(error.to_string()),
        _ => Status::internal(error.to_string()),
    }
}

#[tonic::async_trait]
impl JobService for JobServiceImpl {
    /// 创建 Job：查找 Flow → 创建运行实例 → 注册初始状态的定时器
    async fn create_job(
        &self,
        request: Request<CreateJobRequest>,
    ) -> Result<Response<CreateJobResponse>, Status> {
        let req = request.into_inner();
        if req.max_lifetime_ms == Some(0) {
            return Err(Status::invalid_argument(
                "`max_lifetime_ms` must be greater than 0",
            ));
        }
        let flow = self
            .flow_registration(&req.flow_id)
            .await
            .map_err(|_| Status::not_found(format!("flow `{}` not found", req.flow_id)))?;
        let deadline = req
            .max_lifetime_ms
            .map(|lifetime_ms| Self::now_ms().saturating_add(lifetime_ms));
        let initial_state = flow
            .manifest
            .states
            .iter()
            .find(|state| state.name == flow.manifest.initial_state)
            .ok_or_else(|| Status::internal("initial state not found in manifest"))?;
        let initial_timeouts = if initial_state.kind == StateKind::Terminal {
            Vec::new()
        } else {
            flow.manifest
                .transitions
                .iter()
                .filter(|transition| transition.from == flow.manifest.initial_state)
                .filter_map(|transition| transition.timeout.clone())
                .collect::<Vec<_>>()
        };

        let job = self
            .state
            .job_manager
            .create_job_with_options(
                &flow.flow_id,
                flow.version,
                &flow.manifest.initial_state,
                req.context,
                JobCreationOptions {
                    max_lifetime_ms: req.max_lifetime_ms,
                    lifetime_deadline_ms: deadline,
                    scheduled_timeouts: initial_timeouts
                        .iter()
                        .map(|timeout| ScheduledTimeout {
                            event: timeout.timeout_event.clone(),
                            remaining_ms: timeout.duration_ms,
                        })
                        .collect(),
                },
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let mut action_failures = Vec::new();
        if let Some(on_enter) = initial_state.on_enter.as_deref() {
            let outcome = self
                .run_declared_action(
                    &flow,
                    on_enter,
                    job.id,
                    &flow.manifest.initial_state,
                    job.context.clone(),
                    None,
                )
                .await?;
            if let Some(message) = outcome.warning {
                action_failures.push(message);
            }
        }

        if initial_state.kind == StateKind::Terminal {
            self.state
                .job_manager
                .complete_job(job.id, &flow.manifest.initial_state)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
        } else {
            self.rearm_persisted_timeouts(job.id).await?;
            self.register_lifetime_timer_if_needed(&job).await;
        }

        tracing::info!(job_id = %job.id, flow_id = req.flow_id, "job created");
        if !action_failures.is_empty() {
            tracing::warn!(
                job_id = %job.id,
                failures = ?action_failures,
                "job created with initial action failures"
            );
        }
        Ok(Response::new(CreateJobResponse {
            job_id: job.id.to_string(),
        }))
    }

    async fn get_job(
        &self,
        request: Request<GetJobRequest>,
    ) -> Result<Response<GetJobResponse>, Status> {
        let job_id = parse_uuid(&request.into_inner().job_id)?;
        let job = self.load_job(job_id).await?;

        Ok(Response::new(Self::job_response(&job)))
    }

    async fn list_jobs(
        &self,
        request: Request<ListJobsRequest>,
    ) -> Result<Response<ListJobsResponse>, Status> {
        let flow_id = request.into_inner().flow_id;
        let jobs = self
            .state
            .job_manager
            .list_jobs(&flow_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let resp = jobs.iter().map(Self::job_response).collect();

        Ok(Response::new(ListJobsResponse { jobs: resp }))
    }

    async fn list_all_jobs(
        &self,
        _request: Request<ListAllJobsRequest>,
    ) -> Result<Response<ListJobsResponse>, Status> {
        let jobs = self
            .state
            .job_manager
            .list_all_jobs()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let resp = jobs.iter().map(Self::job_response).collect();

        Ok(Response::new(ListJobsResponse { jobs: resp }))
    }

    async fn delete_job(
        &self,
        request: Request<DeleteJobRequest>,
    ) -> Result<Response<DeleteJobResponse>, Status> {
        let job_id = parse_uuid(&request.into_inner().job_id)?;
        let lock = self.job_lock(job_id).await;
        let _guard = lock.lock().await;

        match self.state.job_manager.delete_job(job_id).await {
            Ok(()) => {}
            Err(error) => {
                if matches!(error, ShirohaError::JobNotFound(_)) {
                    self.remove_job_lock(job_id).await;
                }
                return Err(map_delete_job_error(error));
            }
        }
        self.state.timer_wheel.cancel_all_job_timers(job_id).await;
        self.remove_job_lock(job_id).await;

        Ok(Response::new(DeleteJobResponse {
            job_id: job_id.to_string(),
        }))
    }

    async fn trigger_event(
        &self,
        request: Request<TriggerEventRequest>,
    ) -> Result<Response<TriggerEventResponse>, Status> {
        let req = request.into_inner();
        let job_id = parse_uuid(&req.job_id)?;

        self.enqueue_event(job_id, req.event, req.payload).await?;

        Ok(Response::new(TriggerEventResponse {}))
    }

    async fn pause_job(
        &self,
        request: Request<PauseJobRequest>,
    ) -> Result<Response<PauseJobResponse>, Status> {
        let job_id = parse_uuid(&request.into_inner().job_id)?;
        let lock = self.job_lock(job_id).await;
        let _guard = lock.lock().await;

        match self.state.job_manager.pause_job(job_id).await {
            Ok(()) => {}
            Err(error) => {
                if matches!(error, ShirohaError::JobNotFound(_)) {
                    self.remove_job_lock(job_id).await;
                }
                return Err(map_job_lifecycle_error(error));
            }
        }
        self.state.timer_wheel.pause_job_timers(job_id).await;

        Ok(Response::new(PauseJobResponse {}))
    }

    async fn resume_job(
        &self,
        request: Request<ResumeJobRequest>,
    ) -> Result<Response<ResumeJobResponse>, Status> {
        let job_id = parse_uuid(&request.into_inner().job_id)?;
        let lock = self.job_lock(job_id).await;
        let _guard = lock.lock().await;

        match self.state.job_manager.resume_job(job_id).await {
            Ok(()) => {}
            Err(error) => {
                if matches!(error, ShirohaError::JobNotFound(_)) {
                    self.remove_job_lock(job_id).await;
                }
                return Err(map_job_lifecycle_error(error));
            }
        }
        self.state.timer_wheel.cancel_all_job_timers(job_id).await;
        self.rearm_persisted_timeouts(job_id).await?;
        let job = self.load_job(job_id).await?;
        self.register_lifetime_timer_if_needed(&job).await;
        self.drain_pending_events_locked(job_id).await?;

        Ok(Response::new(ResumeJobResponse {}))
    }

    async fn cancel_job(
        &self,
        request: Request<CancelJobRequest>,
    ) -> Result<Response<CancelJobResponse>, Status> {
        let job_id = parse_uuid(&request.into_inner().job_id)?;
        let lock = self.job_lock(job_id).await;
        let _guard = lock.lock().await;

        match self.state.job_manager.cancel_job(job_id).await {
            Ok(()) => {}
            Err(error) => {
                if matches!(error, ShirohaError::JobNotFound(_)) {
                    self.remove_job_lock(job_id).await;
                }
                return Err(map_job_lifecycle_error(error));
            }
        }
        self.state.timer_wheel.cancel_all_job_timers(job_id).await;
        self.remove_job_lock(job_id).await;

        Ok(Response::new(CancelJobResponse {}))
    }

    /// 查询 Job 的事件溯源日志
    async fn get_job_events(
        &self,
        request: Request<GetJobEventsRequest>,
    ) -> Result<Response<GetJobEventsResponse>, Status> {
        let query = validate_query(request.into_inner())?;

        let events = self
            .state
            .job_manager
            .get_events(query.job_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        let events = filter_events(events, &query)?;

        let records = events
            .iter()
            .map(|e| {
                let kind_json = serde_json::to_string(&e.kind).unwrap_or_default();
                shiroha_proto::shiroha_api::EventRecord {
                    id: e.id.to_string(),
                    job_id: e.job_id.to_string(),
                    timestamp_ms: e.timestamp_ms,
                    kind_json,
                }
            })
            .collect();

        Ok(Response::new(GetJobEventsResponse { events: records }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node_runtime::STANDALONE_NODE_ID;
    use crate::test_support::{
        TestHarness, approval_manifest, approval_manifest_to, deploy_flow, register_flow_version,
        remote_approval_manifest, timeout_manifest, wait_for_job,
    };
    use shiroha_core::event::EventKind;
    use shiroha_core::flow::{
        ActionDef, DispatchMode, FlowManifest, FlowWorld, StateDef, StateKind, TransitionDef,
    };
    use shiroha_proto::shiroha_api::DeleteJobRequest;

    fn initial_on_enter_manifest(flow_id: &str) -> FlowManifest {
        FlowManifest {
            id: flow_id.to_string(),
            host_world: FlowWorld::Sandbox,
            states: vec![StateDef {
                name: "idle".into(),
                kind: StateKind::Normal,
                on_enter: Some("enter".into()),
                on_exit: None,
                subprocess: None,
            }],
            transitions: vec![],
            initial_state: "idle".into(),
            actions: vec![ActionDef {
                name: "enter".into(),
                dispatch: DispatchMode::Local,
                capabilities: Vec::new(),
            }],
        }
    }

    fn state_hooks_manifest(flow_id: &str) -> FlowManifest {
        FlowManifest {
            id: flow_id.to_string(),
            host_world: FlowWorld::Sandbox,
            states: vec![
                StateDef {
                    name: "idle".into(),
                    kind: StateKind::Normal,
                    on_enter: None,
                    on_exit: Some("exit".into()),
                    subprocess: None,
                },
                StateDef {
                    name: "done".into(),
                    kind: StateKind::Terminal,
                    on_enter: Some("enter".into()),
                    on_exit: None,
                    subprocess: None,
                },
            ],
            transitions: vec![TransitionDef {
                from: "idle".into(),
                to: "done".into(),
                event: "approve".into(),
                guard: None,
                action: None,
                timeout: None,
            }],
            initial_state: "idle".into(),
            actions: vec![
                ActionDef {
                    name: "enter".into(),
                    dispatch: DispatchMode::Local,
                    capabilities: Vec::new(),
                },
                ActionDef {
                    name: "exit".into(),
                    dispatch: DispatchMode::Local,
                    capabilities: Vec::new(),
                },
            ],
        }
    }

    fn initial_terminal_manifest(flow_id: &str) -> FlowManifest {
        FlowManifest {
            id: flow_id.to_string(),
            host_world: FlowWorld::Sandbox,
            states: vec![StateDef {
                name: "done".into(),
                kind: StateKind::Terminal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            }],
            transitions: vec![],
            initial_state: "done".into(),
            actions: vec![],
        }
    }

    fn first_guard_rejects_second_transition_accepts(flow_id: &str) -> FlowManifest {
        FlowManifest {
            id: flow_id.to_string(),
            host_world: FlowWorld::Sandbox,
            states: vec![
                StateDef {
                    name: "idle".into(),
                    kind: StateKind::Normal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: "blocked".into(),
                    kind: StateKind::Terminal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: "fallback".into(),
                    kind: StateKind::Terminal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
            ],
            transitions: vec![
                TransitionDef {
                    from: "idle".into(),
                    to: "blocked".into(),
                    event: "approve".into(),
                    guard: Some("deny".into()),
                    action: None,
                    timeout: None,
                },
                TransitionDef {
                    from: "idle".into(),
                    to: "fallback".into(),
                    event: "approve".into(),
                    guard: None,
                    action: None,
                    timeout: None,
                },
            ],
            initial_state: "idle".into(),
            actions: vec![ActionDef {
                name: "deny".into(),
                dispatch: DispatchMode::Local,
                capabilities: Vec::new(),
            }],
        }
    }

    fn context_guard_manifest(flow_id: &str) -> FlowManifest {
        FlowManifest {
            id: flow_id.to_string(),
            host_world: FlowWorld::Sandbox,
            states: vec![
                StateDef {
                    name: "idle".into(),
                    kind: StateKind::Normal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: "done".into(),
                    kind: StateKind::Terminal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
            ],
            transitions: vec![TransitionDef {
                from: "idle".into(),
                to: "done".into(),
                event: "approve".into(),
                guard: Some("require-context".into()),
                action: None,
                timeout: None,
            }],
            initial_state: "idle".into(),
            actions: vec![ActionDef {
                name: "require-context".into(),
                dispatch: DispatchMode::Local,
                capabilities: Vec::new(),
            }],
        }
    }

    fn fanout_follow_up_manifest(flow_id: &str) -> FlowManifest {
        FlowManifest {
            id: flow_id.to_string(),
            host_world: FlowWorld::Sandbox,
            states: vec![
                StateDef {
                    name: "idle".into(),
                    kind: StateKind::Normal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: "collecting".into(),
                    kind: StateKind::Normal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: "done".into(),
                    kind: StateKind::Terminal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: "retry".into(),
                    kind: StateKind::Terminal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
            ],
            transitions: vec![
                TransitionDef {
                    from: "idle".into(),
                    to: "collecting".into(),
                    event: "start".into(),
                    guard: None,
                    action: Some("collect".into()),
                    timeout: None,
                },
                TransitionDef {
                    from: "collecting".into(),
                    to: "done".into(),
                    event: "done".into(),
                    guard: None,
                    action: None,
                    timeout: None,
                },
                TransitionDef {
                    from: "collecting".into(),
                    to: "retry".into(),
                    event: "retry".into(),
                    guard: None,
                    action: None,
                    timeout: None,
                },
            ],
            initial_state: "idle".into(),
            actions: vec![ActionDef {
                name: "collect".into(),
                dispatch: DispatchMode::FanOut(FanOutConfig {
                    strategy: FanOutStrategy::Count(2),
                    aggregator: "pick-success".into(),
                    timeout_ms: None,
                    min_success: None,
                }),
                capabilities: Vec::new(),
            }],
        }
    }

    fn fanout_tagged_manifest(flow_id: &str) -> FlowManifest {
        let mut manifest = fanout_follow_up_manifest(flow_id);
        manifest.actions[0].dispatch = DispatchMode::FanOut(FanOutConfig {
            strategy: FanOutStrategy::Tagged(vec!["edge-a".into(), "edge-b".into()]),
            aggregator: "pick-success".into(),
            timeout_ms: None,
            min_success: None,
        });
        manifest
    }

    fn fanout_timeout_manifest(flow_id: &str) -> FlowManifest {
        let mut manifest = fanout_follow_up_manifest(flow_id);
        manifest.actions[0].name = "slow-collect".into();
        manifest.actions[0].dispatch = DispatchMode::FanOut(FanOutConfig {
            strategy: FanOutStrategy::Count(2),
            aggregator: "pick-success".into(),
            timeout_ms: Some(50),
            min_success: None,
        });
        manifest.transitions[0].action = Some("slow-collect".into());
        manifest
    }

    fn fanout_parallel_manifest(flow_id: &str) -> FlowManifest {
        let mut manifest = fanout_follow_up_manifest(flow_id);
        manifest.actions[0].name = "slow-collect".into();
        manifest.actions[0].dispatch = DispatchMode::FanOut(FanOutConfig {
            strategy: FanOutStrategy::Count(2),
            aggregator: "pick-success".into(),
            timeout_ms: None,
            min_success: None,
        });
        manifest.transitions[0].action = Some("slow-collect".into());
        manifest
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad job lifecycle"]
    async fn create_trigger_and_query_job_lifecycle() {
        let harness = TestHarness::new("job-service-complete").await;
        deploy_flow(
            harness.state.clone(),
            "approval",
            &approval_manifest("approval", Some("allow")),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: Some(vec![1, 2]),
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        let initial = service
            .get_job(Request::new(GetJobRequest {
                job_id: created.job_id.clone(),
            }))
            .await
            .expect("get job")
            .into_inner();
        assert_eq!(initial.state, "running");
        assert_eq!(initial.current_state, "idle");
        assert!(!initial.flow_version.is_empty());
        assert_eq!(initial.context_bytes, Some(2));

        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "approve".into(),
                payload: Some(vec![9]),
            }))
            .await
            .expect("trigger event");

        let final_job = service
            .get_job(Request::new(GetJobRequest {
                job_id: created.job_id.clone(),
            }))
            .await
            .expect("get final job")
            .into_inner();
        assert_eq!(final_job.state, "completed");
        assert_eq!(final_job.current_state, "done");

        let listed = service
            .list_jobs(Request::new(ListJobsRequest {
                flow_id: "approval".into(),
            }))
            .await
            .expect("list jobs")
            .into_inner();
        assert_eq!(listed.jobs.len(), 1);

        let events = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: created.job_id.clone(),
                since_id: None,
                since_timestamp_ms: None,
                limit: None,
                kind: Vec::new(),
            }))
            .await
            .expect("job events")
            .into_inner();
        let kinds: Vec<EventKind> = events
            .events
            .into_iter()
            .map(|record| serde_json::from_str(&record.kind_json).expect("event kind json"))
            .collect();

        assert_eq!(kinds.len(), 4);
        assert!(matches!(kinds[0], EventKind::Created { .. }));
        assert!(matches!(kinds[1], EventKind::Transition { .. }));
        assert!(matches!(kinds[2], EventKind::ActionComplete { .. }));
        assert!(matches!(kinds[3], EventKind::Completed { .. }));
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad job lifecycle"]
    async fn paused_job_queues_events_until_resume() {
        let harness = TestHarness::new("job-service-pause").await;
        deploy_flow(
            harness.state.clone(),
            "approval",
            &approval_manifest("approval", Some("allow")),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();
        let job_id = created.job_id.clone();

        service
            .pause_job(Request::new(PauseJobRequest {
                job_id: job_id.clone(),
            }))
            .await
            .expect("pause job");
        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: job_id.clone(),
                event: "approve".into(),
                payload: None,
            }))
            .await
            .expect("queue paused event");

        let paused = service
            .get_job(Request::new(GetJobRequest {
                job_id: job_id.clone(),
            }))
            .await
            .expect("get paused job")
            .into_inner();
        assert_eq!(paused.state, "paused");
        assert_eq!(paused.current_state, "idle");
        let stored = harness
            .state
            .job_manager
            .get_job(job_id.parse::<Uuid>().expect("uuid"))
            .await
            .expect("get stored job")
            .expect("job exists");
        assert_eq!(stored.pending_events.len(), 1);
        assert_eq!(stored.pending_events[0].event, "approve");

        service
            .resume_job(Request::new(ResumeJobRequest {
                job_id: job_id.clone(),
            }))
            .await
            .expect("resume job");

        let resumed = service
            .get_job(Request::new(GetJobRequest { job_id }))
            .await
            .expect("get resumed job")
            .into_inner();
        assert_eq!(resumed.state, "completed");
        assert_eq!(resumed.current_state, "done");
        let stored = harness
            .state
            .job_manager
            .get_job(resumed.job_id.parse::<Uuid>().expect("uuid"))
            .await
            .expect("get stored job")
            .expect("job exists");
        assert!(stored.pending_events.is_empty());

        let events = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: resumed.job_id.clone(),
                since_id: None,
                since_timestamp_ms: None,
                limit: None,
                kind: Vec::new(),
            }))
            .await
            .expect("job events")
            .into_inner();
        let kinds: Vec<EventKind> = events
            .events
            .into_iter()
            .map(|record| serde_json::from_str(&record.kind_json).expect("event kind json"))
            .collect();
        assert!(matches!(kinds[1], EventKind::Paused));
        assert!(matches!(kinds[2], EventKind::Resumed));
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad job lifetime"]
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
        assert_eq!(job.current_state, "idle");
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad job lifecycle"]
    async fn trigger_event_rejects_guard_without_transition() {
        let harness = TestHarness::new("job-service-guard").await;
        deploy_flow(
            harness.state.clone(),
            "guarded",
            &approval_manifest("guarded", Some("deny")),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "guarded".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        let error = service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "approve".into(),
                payload: None,
            }))
            .await
            .expect_err("guard should reject");
        assert_eq!(error.code(), tonic::Code::FailedPrecondition);

        let job = service
            .get_job(Request::new(GetJobRequest {
                job_id: created.job_id.clone(),
            }))
            .await
            .expect("get job")
            .into_inner();
        assert_eq!(job.state, "running");
        assert_eq!(job.current_state, "idle");

        let events = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: created.job_id,
                since_id: None,
                since_timestamp_ms: None,
                limit: None,
                kind: Vec::new(),
            }))
            .await
            .expect("job events")
            .into_inner();
        assert_eq!(events.events.len(), 1);
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad job lifecycle"]
    async fn delete_job_removes_terminal_job_and_events() {
        let harness = TestHarness::new("job-service-delete").await;
        deploy_flow(
            harness.state.clone(),
            "approval",
            &approval_manifest("approval", Some("allow")),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "approve".into(),
                payload: None,
            }))
            .await
            .expect("trigger event");

        service
            .delete_job(Request::new(DeleteJobRequest {
                job_id: created.job_id.clone(),
            }))
            .await
            .expect("delete job");

        let job_uuid = created.job_id.parse::<Uuid>().expect("uuid");
        assert!(
            harness
                .state
                .job_manager
                .get_job(job_uuid)
                .await
                .expect("get job")
                .is_none()
        );
        assert!(
            harness
                .state
                .job_manager
                .get_events(job_uuid)
                .await
                .expect("get events")
                .is_empty()
        );
        assert!(!harness.state.job_locks.lock().await.contains_key(&job_uuid));
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad job lifecycle"]
    async fn delete_job_rejects_running_job() {
        let harness = TestHarness::new("job-service-delete-running").await;
        deploy_flow(
            harness.state.clone(),
            "approval",
            &approval_manifest("approval", Some("allow")),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        let error = service
            .delete_job(Request::new(DeleteJobRequest {
                job_id: created.job_id,
            }))
            .await
            .expect_err("delete running job");
        assert_eq!(error.code(), tonic::Code::FailedPrecondition);
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad job lifecycle"]
    async fn timer_forwarder_delivers_timeout_event() {
        let harness = TestHarness::with_timer_forwarder("job-service-timer").await;
        deploy_flow(
            harness.state.clone(),
            "timer-flow",
            &timeout_manifest("timer-flow"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "timer-flow".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        let timed_out = wait_for_job(&service, &created.job_id, "completed", "timed_out").await;
        assert_eq!(timed_out.job_id, created.job_id);

        let events = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: created.job_id,
                since_id: None,
                since_timestamp_ms: None,
                limit: None,
                kind: Vec::new(),
            }))
            .await
            .expect("job events")
            .into_inner();
        let kinds: Vec<EventKind> = events
            .events
            .into_iter()
            .map(|record| serde_json::from_str(&record.kind_json).expect("event kind json"))
            .collect();
        assert!(kinds.iter().any(|kind| matches!(kind, EventKind::Completed { final_state } if final_state == "timed_out")));
    }

    #[tokio::test]
    async fn redeploy_keeps_existing_jobs_on_their_bound_flow_version() {
        let harness = TestHarness::new("job-service-version-binding").await;
        let mut first = approval_manifest_to("approval", "done");
        first.transitions[0].action = None;
        first.actions.clear();
        register_flow_version(&harness.state, "approval", Uuid::now_v7(), first).await;

        let service = JobServiceImpl::new(harness.state.clone());
        let old_job = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create old job")
            .into_inner();

        let mut second = approval_manifest_to("approval", "rerouted");
        second.transitions[0].action = None;
        second.actions.clear();
        register_flow_version(&harness.state, "approval", Uuid::now_v7(), second).await;

        let new_job = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create new job")
            .into_inner();

        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: old_job.job_id.clone(),
                event: "approve".into(),
                payload: None,
            }))
            .await
            .expect("trigger old job");
        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: new_job.job_id.clone(),
                event: "approve".into(),
                payload: None,
            }))
            .await
            .expect("trigger new job");

        let old_state = wait_for_job(&service, &old_job.job_id, "completed", "done").await;
        let new_state = wait_for_job(&service, &new_job.job_id, "completed", "rerouted").await;

        assert_eq!(old_state.current_state, "done");
        assert_eq!(new_state.current_state, "rerouted");
    }

    #[tokio::test]
    async fn list_all_jobs_returns_jobs_across_flows() {
        let harness = TestHarness::new("job-service-list-all").await;
        register_flow_version(
            &harness.state,
            "alpha",
            Uuid::now_v7(),
            approval_manifest_to("alpha", "done"),
        )
        .await;
        register_flow_version(
            &harness.state,
            "beta",
            Uuid::now_v7(),
            approval_manifest_to("beta", "done"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "alpha".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create alpha job");
        service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "beta".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create beta job");

        let listed = service
            .list_all_jobs(Request::new(ListAllJobsRequest {}))
            .await
            .expect("list all jobs")
            .into_inner();

        assert_eq!(listed.jobs.len(), 2);
        assert!(listed.jobs.iter().any(|job| job.flow_id == "alpha"));
        assert!(listed.jobs.iter().any(|job| job.flow_id == "beta"));
    }

    #[tokio::test]
    async fn get_job_exposes_lifetime_fields() {
        let harness = TestHarness::new("job-service-lifetime-observable").await;
        register_flow_version(
            &harness.state,
            "approval",
            Uuid::now_v7(),
            approval_manifest_to("approval", "done"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: None,
                max_lifetime_ms: Some(500),
            }))
            .await
            .expect("create job")
            .into_inner();

        let job = service
            .get_job(Request::new(GetJobRequest {
                job_id: created.job_id,
            }))
            .await
            .expect("get job")
            .into_inner();

        assert_eq!(job.max_lifetime_ms, Some(500));
        assert!(job.lifetime_deadline_ms.is_some());
        assert!(job.remaining_lifetime_ms.is_some());
    }

    #[tokio::test]
    async fn later_transition_can_win_after_earlier_guard_rejects() {
        let harness = TestHarness::new("job-service-guard-fallback").await;
        deploy_flow(
            harness.state.clone(),
            "approval",
            &first_guard_rejects_second_transition_accepts("approval"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "approve".into(),
                payload: None,
            }))
            .await
            .expect("trigger fallback transition");

        let final_job = wait_for_job(&service, &created.job_id, "completed", "fallback").await;
        assert_eq!(final_job.current_state, "fallback");
    }

    #[tokio::test]
    async fn guard_can_see_persisted_job_context() {
        let harness = TestHarness::new("job-service-context-guard").await;
        deploy_flow(
            harness.state.clone(),
            "approval",
            &context_guard_manifest("approval"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: Some(b"context-present".to_vec()),
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "approve".into(),
                payload: None,
            }))
            .await
            .expect("trigger event");

        let final_job = wait_for_job(&service, &created.job_id, "completed", "done").await;
        assert_eq!(final_job.current_state, "done");
    }

    #[tokio::test]
    async fn create_job_succeeds_even_if_initial_hook_cannot_execute() {
        let harness = TestHarness::new("job-service-initial-hook-failure").await;
        register_flow_version(
            &harness.state,
            "approval",
            Uuid::now_v7(),
            initial_on_enter_manifest("approval"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        let events = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: created.job_id.clone(),
                since_id: None,
                since_timestamp_ms: None,
                limit: None,
                kind: Vec::new(),
            }))
            .await
            .expect("job events")
            .into_inner();
        let kinds: Vec<EventKind> = events
            .events
            .into_iter()
            .map(|record| serde_json::from_str(&record.kind_json).expect("event kind json"))
            .collect();
        assert!(matches!(kinds[0], EventKind::Created { .. }));
        assert!(matches!(
            &kinds[1],
            EventKind::ActionComplete { action, status, .. }
                if action == "enter" && *status == ExecutionStatus::Failed
        ));
    }

    #[tokio::test]
    async fn trigger_event_succeeds_even_if_post_transition_action_cannot_execute() {
        let harness = TestHarness::new("job-service-action-failure-after-commit").await;
        register_flow_version(
            &harness.state,
            "approval",
            Uuid::now_v7(),
            approval_manifest_to("approval", "done"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "approve".into(),
                payload: None,
            }))
            .await
            .expect("trigger event should still succeed");

        let final_job = wait_for_job(&service, &created.job_id, "completed", "done").await;
        assert_eq!(final_job.current_state, "done");

        let events = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: created.job_id,
                since_id: None,
                since_timestamp_ms: None,
                limit: None,
                kind: Vec::new(),
            }))
            .await
            .expect("job events")
            .into_inner();
        let kinds: Vec<EventKind> = events
            .events
            .into_iter()
            .map(|record| serde_json::from_str(&record.kind_json).expect("event kind json"))
            .collect();
        assert!(matches!(kinds[1], EventKind::Transition { .. }));
        assert!(matches!(
            &kinds[2],
            EventKind::ActionComplete { action, status, .. }
                if action == "ship" && *status == ExecutionStatus::Failed
        ));
        assert!(matches!(kinds[3], EventKind::Completed { .. }));
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad remote dispatch"]
    async fn remote_dispatch_requires_registered_transport_node() {
        let harness = TestHarness::new("job-service-remote-transport").await;
        deploy_flow(
            harness.state.clone(),
            "remote-approval",
            &remote_approval_manifest("remote-approval", Some("allow")),
        )
        .await;

        harness
            .state
            .transport
            .unregister_node(STANDALONE_NODE_ID)
            .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let flow = service
            .flow_registration("remote-approval")
            .await
            .expect("flow registration");

        let error = service
            .invoke_action(
                &flow,
                "ship",
                Uuid::now_v7(),
                "idle",
                None,
                Some(b"transport-check".to_vec()),
            )
            .await
            .expect_err("remote dispatch should require a registered node");

        assert_eq!(error.code(), tonic::Code::Internal);
        assert!(error.message().contains("node `standalone` not registered"));
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad remote dispatch"]
    async fn remote_dispatch_completes_job_through_transport() {
        let harness = TestHarness::new("job-service-remote-success").await;
        deploy_flow(
            harness.state.clone(),
            "remote-approval",
            &remote_approval_manifest("remote-approval", Some("allow")),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "remote-approval".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "approve".into(),
                payload: Some(b"remote-payload".to_vec()),
            }))
            .await
            .expect("trigger remote event");

        let final_job = wait_for_job(&service, &created.job_id, "completed", "done").await;
        assert_eq!(final_job.current_state, "done");

        let events = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: created.job_id,
                since_id: None,
                since_timestamp_ms: None,
                limit: None,
                kind: Vec::new(),
            }))
            .await
            .expect("job events")
            .into_inner();
        let kinds: Vec<EventKind> = events
            .events
            .into_iter()
            .map(|record| serde_json::from_str(&record.kind_json).expect("event kind json"))
            .collect();

        assert!(matches!(kinds[0], EventKind::Created { .. }));
        assert!(matches!(kinds[1], EventKind::Transition { .. }));
        assert!(matches!(
            &kinds[2],
            EventKind::ActionComplete { action, status, .. }
                if action == "ship" && *status == ExecutionStatus::Success
        ));
        assert!(matches!(kinds[3], EventKind::Completed { .. }));
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad fan-out dispatch"]
    async fn fanout_dispatch_aggregates_and_drives_follow_up_transition() {
        let harness = TestHarness::new("job-service-fanout-follow-up").await;
        deploy_flow(
            harness.state.clone(),
            "fanout-flow",
            &fanout_follow_up_manifest("fanout-flow"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "fanout-flow".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "start".into(),
                payload: Some(b"fanout-payload".to_vec()),
            }))
            .await
            .expect("trigger fanout");

        let final_job = wait_for_job(&service, &created.job_id, "completed", "done").await;
        assert_eq!(final_job.current_state, "done");

        let events = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: created.job_id,
                since_id: None,
                since_timestamp_ms: None,
                limit: None,
                kind: Vec::new(),
            }))
            .await
            .expect("job events")
            .into_inner();
        let kinds: Vec<EventKind> = events
            .events
            .into_iter()
            .map(|record| serde_json::from_str(&record.kind_json).expect("event kind json"))
            .collect();
        let action_complete_nodes = kinds
            .iter()
            .filter_map(|kind| match kind {
                EventKind::ActionComplete {
                    action,
                    node_id,
                    status,
                } if action == "collect" && *status == ExecutionStatus::Success => node_id.clone(),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(matches!(kinds[0], EventKind::Created { .. }));
        assert!(matches!(
            &kinds[1],
            EventKind::Transition { event, from, to, .. }
                if event == "start" && from == "idle" && to == "collecting"
        ));
        assert_eq!(action_complete_nodes.len(), 2);
        assert!(action_complete_nodes.contains(&"standalone-1".to_string()));
        assert!(action_complete_nodes.contains(&"standalone-2".to_string()));
        assert!(matches!(
            kinds.iter().find(|kind| matches!(kind, EventKind::Transition { event, .. } if event == "done")).expect("done transition"),
            EventKind::Transition { event, from, to, .. }
                if event == "done" && from == "collecting" && to == "done"
        ));
        assert!(matches!(kinds.last(), Some(EventKind::Completed { .. })));
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad fan-out dispatch"]
    async fn fanout_tagged_strategy_records_tagged_node_ids() {
        let harness = TestHarness::new("job-service-fanout-tagged").await;
        deploy_flow(
            harness.state.clone(),
            "fanout-tagged",
            &fanout_tagged_manifest("fanout-tagged"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "fanout-tagged".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "start".into(),
                payload: Some(b"fanout-tagged".to_vec()),
            }))
            .await
            .expect("trigger tagged fanout");

        let _final_job = wait_for_job(&service, &created.job_id, "completed", "done").await;
        let events = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: created.job_id,
                since_id: None,
                since_timestamp_ms: None,
                limit: None,
                kind: Vec::new(),
            }))
            .await
            .expect("job events")
            .into_inner();
        let kinds: Vec<EventKind> = events
            .events
            .into_iter()
            .map(|record| serde_json::from_str(&record.kind_json).expect("event kind json"))
            .collect();
        let nodes = kinds
            .iter()
            .filter_map(|kind| match kind {
                EventKind::ActionComplete { node_id, .. } => node_id.clone(),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert!(nodes.contains(&"edge-a".to_string()));
        assert!(nodes.contains(&"edge-b".to_string()));
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad fan-out dispatch"]
    async fn fanout_timeout_aggregates_partial_results() {
        let harness = TestHarness::new("job-service-fanout-timeout").await;
        deploy_flow(
            harness.state.clone(),
            "fanout-timeout",
            &fanout_timeout_manifest("fanout-timeout"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "fanout-timeout".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "start".into(),
                payload: Some(b"fanout-timeout".to_vec()),
            }))
            .await
            .expect("trigger fanout timeout");

        let final_job = wait_for_job(&service, &created.job_id, "completed", "retry").await;
        assert_eq!(final_job.current_state, "retry");
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad fan-out dispatch"]
    async fn fanout_runs_slots_in_parallel() {
        let harness = TestHarness::new("job-service-fanout-parallel").await;
        deploy_flow(
            harness.state.clone(),
            "fanout-parallel",
            &fanout_parallel_manifest("fanout-parallel"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "fanout-parallel".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        let started_at = tokio::time::Instant::now();
        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "start".into(),
                payload: Some(b"fanout-parallel".to_vec()),
            }))
            .await
            .expect("trigger parallel fanout");

        let _final_job = wait_for_job(&service, &created.job_id, "completed", "done").await;
        let elapsed = started_at.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(260),
            "fan-out should complete in parallel, elapsed={elapsed:?}"
        );
    }

    #[tokio::test]
    async fn missing_job_request_does_not_leave_job_lock_behind() {
        let harness = TestHarness::new("job-service-missing-job-lock-cleanup").await;
        let service = JobServiceImpl::new(harness.state.clone());
        let missing_job_id = Uuid::now_v7();

        let error = service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: missing_job_id.to_string(),
                event: "approve".into(),
                payload: None,
            }))
            .await
            .expect_err("missing job should fail");

        assert_eq!(error.code(), tonic::Code::NotFound);
        assert!(
            !harness
                .state
                .job_locks
                .lock()
                .await
                .contains_key(&missing_job_id)
        );
    }

    #[tokio::test]
    async fn create_job_immediately_completes_when_initial_state_is_terminal() {
        let harness = TestHarness::new("job-service-terminal-initial").await;
        register_flow_version(
            &harness.state,
            "terminal",
            Uuid::now_v7(),
            initial_terminal_manifest("terminal"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "terminal".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        let job = service
            .get_job(Request::new(GetJobRequest {
                job_id: created.job_id.clone(),
            }))
            .await
            .expect("get job")
            .into_inner();
        assert_eq!(job.state, "completed");
        assert_eq!(job.current_state, "done");

        let events = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: created.job_id,
                since_id: None,
                since_timestamp_ms: None,
                limit: None,
                kind: Vec::new(),
            }))
            .await
            .expect("job events")
            .into_inner();
        let kinds: Vec<EventKind> = events
            .events
            .into_iter()
            .map(|record| serde_json::from_str(&record.kind_json).expect("event kind json"))
            .collect();
        assert_eq!(kinds.len(), 2);
        assert!(matches!(kinds[0], EventKind::Created { .. }));
        assert!(matches!(
            &kinds[1],
            EventKind::Completed { final_state } if final_state == "done"
        ));
    }

    #[tokio::test]
    async fn terminal_job_releases_job_lock() {
        let harness = TestHarness::new("job-service-release-lock-on-complete").await;
        register_flow_version(
            &harness.state,
            "approval",
            Uuid::now_v7(),
            approval_manifest_to("approval", "done"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();
        let job_uuid = created.job_id.parse::<Uuid>().expect("uuid");

        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "approve".into(),
                payload: None,
            }))
            .await
            .expect("trigger event");

        let _ = wait_for_job(&service, &created.job_id, "completed", "done").await;
        assert!(!harness.state.job_locks.lock().await.contains_key(&job_uuid));
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad job lifecycle"]
    async fn create_job_runs_initial_state_on_enter_action() {
        let harness = TestHarness::new("job-service-initial-on-enter").await;
        deploy_flow(
            harness.state.clone(),
            "with-initial-hook",
            &initial_on_enter_manifest("with-initial-hook"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "with-initial-hook".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        let events = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: created.job_id,
                since_id: None,
                since_timestamp_ms: None,
                limit: None,
                kind: Vec::new(),
            }))
            .await
            .expect("job events")
            .into_inner();
        let kinds: Vec<EventKind> = events
            .events
            .into_iter()
            .map(|record| serde_json::from_str(&record.kind_json).expect("event kind json"))
            .collect();

        assert_eq!(kinds.len(), 2);
        assert!(matches!(kinds[0], EventKind::Created { .. }));
        assert!(matches!(
            &kinds[1],
            EventKind::ActionComplete { action, .. } if action == "enter"
        ));
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad job lifecycle"]
    async fn transition_runs_state_exit_and_enter_actions() {
        let harness = TestHarness::new("job-service-state-hooks").await;
        deploy_flow(
            harness.state.clone(),
            "with-hooks",
            &state_hooks_manifest("with-hooks"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "with-hooks".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "approve".into(),
                payload: None,
            }))
            .await
            .expect("trigger event");

        let events = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: created.job_id,
                since_id: None,
                since_timestamp_ms: None,
                limit: None,
                kind: Vec::new(),
            }))
            .await
            .expect("job events")
            .into_inner();
        let kinds: Vec<EventKind> = events
            .events
            .into_iter()
            .map(|record| serde_json::from_str(&record.kind_json).expect("event kind json"))
            .collect();

        assert_eq!(kinds.len(), 5);
        assert!(matches!(kinds[0], EventKind::Created { .. }));
        assert!(matches!(kinds[1], EventKind::Transition { .. }));
        assert!(matches!(
            &kinds[2],
            EventKind::ActionComplete { action, .. } if action == "exit"
        ));
        assert!(matches!(
            &kinds[3],
            EventKind::ActionComplete { action, .. } if action == "enter"
        ));
        assert!(matches!(kinds[4], EventKind::Completed { .. }));
    }

    #[tokio::test]
    #[ignore = "heavy job integration smoke; run explicitly when validating shirohad job lifecycle"]
    async fn get_job_events_supports_cursor_kind_and_limit() {
        let harness = TestHarness::new("job-service-events-query").await;
        deploy_flow(
            harness.state.clone(),
            "approval",
            &approval_manifest("approval", Some("allow")),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let created = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "approve".into(),
                payload: None,
            }))
            .await
            .expect("trigger event");

        let all_events = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: created.job_id.clone(),
                since_id: None,
                since_timestamp_ms: None,
                limit: None,
                kind: Vec::new(),
            }))
            .await
            .expect("all events")
            .into_inner()
            .events;
        assert_eq!(all_events.len(), 4);

        let filtered = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: created.job_id,
                since_id: Some(all_events[0].id.clone()),
                since_timestamp_ms: None,
                limit: Some(1),
                kind: vec!["transition".into()],
            }))
            .await
            .expect("filtered events")
            .into_inner()
            .events;
        assert_eq!(filtered.len(), 1);
        let kind: EventKind =
            serde_json::from_str(&filtered[0].kind_json).expect("event kind json");
        assert!(matches!(kind, EventKind::Transition { .. }));
    }
}
