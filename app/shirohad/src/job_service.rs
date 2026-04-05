//! gRPC JobService 实现
//!
//! 处理 Job 的创建、状态查询、事件触发、生命周期管理（暂停/恢复/取消）。
//! Phase 1 里所有事件都按 Job 串行处理，暂停期间事件会暂存在内存队列中。

use std::sync::Arc;

use shiroha_core::error::ShirohaError;
use shiroha_core::event::EventKind;
use shiroha_core::flow::{DispatchMode, FlowRegistration, TimeoutDef};
use shiroha_core::job::{ActionResult, ExecutionStatus, Job, JobState};
use shiroha_proto::shiroha_api::job_service_server::JobService;
use shiroha_proto::shiroha_api::{
    CancelJobRequest, CancelJobResponse, CreateJobRequest, CreateJobResponse, DeleteJobRequest,
    DeleteJobResponse, GetJobEventsRequest, GetJobEventsResponse, GetJobRequest, GetJobResponse,
    ListJobsRequest, ListJobsResponse, PauseJobRequest, PauseJobResponse, ResumeJobRequest,
    ResumeJobResponse, TriggerEventRequest, TriggerEventResponse,
};
use shiroha_wasm::host::{ActionContext, GuardContext, WasmHost};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::server::ShirohaState;

/// JobService 的 standalone 实现。
///
/// 它把 gRPC 请求、定时器回调和 WASM 调用串成一条统一的 Job 处理链路。
pub struct JobServiceImpl {
    state: Arc<ShirohaState>,
}

impl JobServiceImpl {
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
        self.handle_event_locked(job_id, event, payload).await
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
            .flows
            .lock()
            .await
            .get(flow_id)
            .cloned()
            .ok_or_else(|| Status::internal(format!("flow `{flow_id}` not loaded in memory")))
    }

    async fn flow_registration_for_job(&self, job: &Job) -> Result<FlowRegistration, Status> {
        self.state
            .flow_versions
            .lock()
            .await
            .get(&(job.flow_id.clone(), job.flow_version))
            .cloned()
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
        let job = self.load_job(job_id).await?;
        match job.state {
            JobState::Running => self.process_running_event(job, event, payload).await,
            JobState::Paused => {
                // 暂停态不丢事件，而是持久化到 Job 快照里，恢复后继续按顺序处理。
                self.state
                    .job_manager
                    .queue_pending_event(job_id, event.clone(), payload)
                    .await
                    .map_err(|e| Status::internal(e.to_string()))?;
                tracing::info!(job_id = %job_id, event, "job paused; queued event");
                Ok(())
            }
            JobState::Cancelled | JobState::Completed => {
                Err(Status::failed_precondition(format!("job is {}", job.state)))
            }
        }
    }

    async fn process_running_event(
        &self,
        job: Job,
        event: String,
        payload: Option<Vec<u8>>,
    ) -> Result<(), Status> {
        let flow = self.flow_registration_for_job(&job).await?;
        // 只在 engine 锁里完成“查拓扑、算下一步”这类纯读操作，
        // 拿到执行计划后立刻释放锁，避免后续 WASM 调用阻塞其他 Job 读取同一个 Flow。
        let (from, to, action, guard, on_exit, on_enter, is_terminal, timeouts) = {
            let engines = self.state.versioned_engines.lock().await;
            let engine = engines
                .get(&(job.flow_id.clone(), job.flow_version))
                .ok_or_else(|| Status::internal("engine not found for flow"))?;
            let result = engine
                .process_event(&job.current_state, &event)
                .map_err(|e| Status::failed_precondition(e.to_string()))?;
            let from_state = engine
                .get_state(&result.from)
                .ok_or_else(|| Status::internal("source state not found in manifest"))?;
            let to_state = engine
                .get_state(&result.to)
                .ok_or_else(|| Status::internal("target state not found in manifest"))?;
            let next_timeouts = if engine.is_terminal(&result.to) {
                Vec::new()
            } else {
                engine
                    .manifest()
                    .transitions
                    .iter()
                    .filter(|t| t.from == result.to)
                    .filter_map(|t| t.timeout.clone())
                    .collect()
            };

            (
                result.from,
                result.to.clone(),
                result.action,
                result.guard,
                from_state.on_exit.clone(),
                to_state.on_enter.clone(),
                engine.is_terminal(&result.to),
                next_timeouts,
            )
        };

        if let Some(guard_name) = guard.as_deref() {
            // guard 失败时不能提交转移，所以必须先于 transition_job 执行。
            let guard_ctx = GuardContext {
                job_id: job.id.to_string(),
                from_state: from.clone(),
                to_state: to.clone(),
                event: event.clone(),
                payload: payload.clone(),
            };
            let allowed = self.invoke_guard(&flow, guard_name, guard_ctx).await?;
            if !allowed {
                return Err(Status::failed_precondition(
                    ShirohaError::GuardRejected.to_string(),
                ));
            }
        }

        // 一旦离开旧状态，旧状态上的 timeout 全部失效，因此先整体撤销再按新状态重建。
        self.state.timer_wheel.cancel_all_job_timers(job.id).await;
        self.state
            .job_manager
            .transition_job(job.id, &event, &from, &to, action.clone())
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let mut action_failures = Vec::new();
        for (action_name, action_state) in [
            on_exit.as_deref().map(|name| (name, from.as_str())),
            action.as_deref().map(|name| (name, to.as_str())),
            on_enter.as_deref().map(|name| (name, to.as_str())),
        ]
        .into_iter()
        .flatten()
        {
            if let Some(message) = self
                .run_declared_action(&flow, action_name, job.id, action_state, payload.clone())
                .await?
            {
                action_failures.push(message);
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
        } else {
            // timeout 是“状态出边”的属性，所以进入新状态后要重新扫描所有出边注册。
            for timeout in timeouts {
                self.register_timeout(job.id, timeout).await;
            }
        }

        tracing::info!(job_id = %job.id, event, from, to, "event processed");

        if !action_failures.is_empty() {
            return Err(Status::aborted(action_failures.join("; ")));
        }

        Ok(())
    }

    async fn drain_pending_events_locked(&self, job_id: Uuid) -> Result<(), Status> {
        while let Some(queued) = self
            .state
            .job_manager
            .pop_pending_event(job_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
        {
            // 每消费一个排队事件都重新读取 Job，确保后续事件看到的是最新状态。
            let job = self.load_job(job_id).await?;
            match job.state {
                JobState::Running => {
                    self.process_running_event(job, queued.event, queued.payload)
                        .await?;
                }
                JobState::Paused => {
                    self.state
                        .job_manager
                        .push_front_pending_event(job_id, queued)
                        .await
                        .map_err(|e| Status::internal(e.to_string()))?;
                    return Ok(());
                }
                JobState::Cancelled | JobState::Completed => {
                    self.state
                        .job_manager
                        .clear_pending_events(job_id)
                        .await
                        .map_err(|e| Status::internal(e.to_string()))?;
                    return Ok(());
                }
            }
        }

        Ok(())
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

    async fn run_declared_action(
        &self,
        flow: &FlowRegistration,
        action_name: &str,
        job_id: Uuid,
        state: &str,
        payload: Option<Vec<u8>>,
    ) -> Result<Option<String>, Status> {
        // 当前语义里 action / state hook 都属于“状态推进后的副作用”：
        // 即使执行状态失败，转移和事件日志也已经提交，便于后续审计/补偿。
        let action_result = self
            .invoke_action(flow, action_name, job_id, state, payload)
            .await?;
        self.record_action_result(job_id, action_name, &action_result)
            .await?;
        if action_result.status == ExecutionStatus::Success {
            return Ok(None);
        }

        Ok(Some(format!(
            "transition committed but action `{action_name}` finished with status {}",
            execution_status_name(action_result.status)
        )))
    }

    async fn register_timeout(&self, job_id: Uuid, timeout: TimeoutDef) {
        self.state
            .timer_wheel
            .register(
                job_id,
                timeout.timeout_event,
                std::time::Duration::from_millis(timeout.duration_ms),
            )
            .await;
    }

    fn wasm_host(&self, flow: &FlowRegistration) -> Result<WasmHost, Status> {
        let module = self
            .state
            .module_cache
            .get(&flow.wasm_hash)
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "WASM module for flow `{}` is not available in cache; redeploy the flow in this process",
                    flow.flow_id
                ))
            })?;

        // 缓存中保存的是编译后的 component；每次调用仍会创建新的 guest 实例。
        WasmHost::new(self.state.wasm_runtime.engine(), module.component())
            .map_err(|e| Status::internal(e.to_string()))
    }

    async fn invoke_guard(
        &self,
        flow: &FlowRegistration,
        guard_name: &str,
        ctx: GuardContext,
    ) -> Result<bool, Status> {
        let mut host = self.wasm_host(flow)?;
        host.invoke_guard(guard_name, ctx)
            .map_err(|e| Status::internal(e.to_string()))
    }

    async fn invoke_action(
        &self,
        flow: &FlowRegistration,
        action_name: &str,
        job_id: Uuid,
        state: &str,
        payload: Option<Vec<u8>>,
    ) -> Result<ActionResult, Status> {
        let dispatch = flow
            .manifest
            .actions
            .iter()
            .find(|candidate| candidate.name == action_name)
            .map(|candidate| &candidate.dispatch)
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "action `{action_name}` not declared in manifest"
                ))
            })?;

        match dispatch {
            DispatchMode::Local | DispatchMode::Remote => {
                // standalone 模式下本地执行和远端执行暂时走同一条 WASM 调用路径；
                // 真正的集群调度会在这里分叉。
                let mut host = self.wasm_host(flow)?;
                host.invoke_action(
                    action_name,
                    ActionContext {
                        job_id: job_id.to_string(),
                        state: state.to_string(),
                        payload,
                    },
                )
                .map_err(|e| Status::internal(e.to_string()))
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

fn parse_uuid(s: &str) -> Result<Uuid, Status> {
    s.parse::<Uuid>()
        .map_err(|_| Status::invalid_argument(format!("invalid UUID: {s}")))
}

fn event_kind_name(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::Created { .. } => "created",
        EventKind::Transition { .. } => "transition",
        EventKind::ActionComplete { .. } => "action_complete",
        EventKind::Paused => "paused",
        EventKind::Resumed => "resumed",
        EventKind::Cancelled => "cancelled",
        EventKind::Completed { .. } => "completed",
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
        let flow = self
            .flow_registration(&req.flow_id)
            .await
            .map_err(|_| Status::not_found(format!("flow `{}` not found", req.flow_id)))?;

        let job = self
            .state
            .job_manager
            .create_job(
                &flow.flow_id,
                flow.version,
                &flow.manifest.initial_state,
                req.context,
            )
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let mut action_failures = Vec::new();
        let initial_state = flow
            .manifest
            .states
            .iter()
            .find(|state| state.name == flow.manifest.initial_state)
            .ok_or_else(|| Status::internal("initial state not found in manifest"))?;
        if let Some(on_enter) = initial_state.on_enter.as_deref()
            && let Some(message) = self
                .run_declared_action(&flow, on_enter, job.id, &flow.manifest.initial_state, None)
                .await?
        {
            action_failures.push(message);
        }

        for transition in &flow.manifest.transitions {
            if transition.from == flow.manifest.initial_state
                && let Some(timeout) = transition.timeout.clone()
            {
                self.register_timeout(job.id, timeout).await;
            }
        }

        tracing::info!(job_id = %job.id, flow_id = req.flow_id, "job created");
        if !action_failures.is_empty() {
            return Err(Status::aborted(action_failures.join("; ")));
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

    async fn delete_job(
        &self,
        request: Request<DeleteJobRequest>,
    ) -> Result<Response<DeleteJobResponse>, Status> {
        let job_id = parse_uuid(&request.into_inner().job_id)?;
        let lock = self.job_lock(job_id).await;
        let _guard = lock.lock().await;

        self.state
            .job_manager
            .delete_job(job_id)
            .await
            .map_err(|e| match e {
                ShirohaError::JobNotFound(_) => Status::not_found(e.to_string()),
                ShirohaError::InvalidJobState { .. } => Status::failed_precondition(e.to_string()),
                _ => Status::internal(e.to_string()),
            })?;
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

        self.state
            .job_manager
            .pause_job(job_id)
            .await
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
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

        self.state
            .job_manager
            .resume_job(job_id)
            .await
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        self.state.timer_wheel.resume_job_timers(job_id).await;
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

        self.state
            .job_manager
            .cancel_job(job_id)
            .await
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        self.state.timer_wheel.cancel_all_job_timers(job_id).await;

        Ok(Response::new(CancelJobResponse {}))
    }

    /// 查询 Job 的事件溯源日志
    async fn get_job_events(
        &self,
        request: Request<GetJobEventsRequest>,
    ) -> Result<Response<GetJobEventsResponse>, Status> {
        let req = request.into_inner();
        let job_id = parse_uuid(&req.job_id)?;
        if req.since_id.is_some() && req.since_timestamp_ms.is_some() {
            return Err(Status::invalid_argument(
                "`since_id` and `since_timestamp_ms` cannot be used together",
            ));
        }
        if req.limit == Some(0) {
            return Err(Status::invalid_argument("`limit` must be greater than 0"));
        }
        if let Some(kind) = req.kind.iter().find(|kind| {
            !matches!(
                kind.as_str(),
                "created"
                    | "transition"
                    | "action_complete"
                    | "paused"
                    | "resumed"
                    | "cancelled"
                    | "completed"
            )
        }) {
            return Err(Status::invalid_argument(format!(
                "unknown event kind filter: {kind}"
            )));
        }

        let mut events = self
            .state
            .job_manager
            .get_events(job_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        if let Some(since_id) = req.since_id {
            let cursor = parse_uuid(&since_id)?;
            let Some(index) = events.iter().position(|event| event.id == cursor) else {
                return Err(Status::invalid_argument(format!(
                    "event `{since_id}` not found for job `{}`",
                    req.job_id
                )));
            };
            events.drain(..=index);
        }
        if let Some(since_timestamp_ms) = req.since_timestamp_ms {
            events.retain(|event| event.timestamp_ms > since_timestamp_ms);
        }
        if !req.kind.is_empty() {
            let kinds = req
                .kind
                .iter()
                .map(String::as_str)
                .collect::<std::collections::HashSet<_>>();
            events.retain(|event| kinds.contains(event_kind_name(&event.kind)));
        }
        if let Some(limit) = req.limit {
            events.truncate(limit as usize);
        }

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
    use tokio::time::{Duration, sleep, timeout};

    use super::*;
    use crate::flow_service::FlowServiceImpl;
    use crate::test_support::{
        TestHarness, approval_manifest, timeout_manifest, wasm_for_manifest,
    };
    use shiroha_core::event::EventKind;
    use shiroha_core::flow::{
        ActionDef, DispatchMode, FlowManifest, StateDef, StateKind, TransitionDef,
    };
    use shiroha_proto::shiroha_api::DeleteJobRequest;
    use shiroha_proto::shiroha_api::flow_service_server::FlowService;

    async fn deploy_flow(
        state: Arc<ShirohaState>,
        flow_id: &str,
        manifest: &shiroha_core::flow::FlowManifest,
    ) {
        let flow_service = FlowServiceImpl::new(state);
        flow_service
            .deploy_flow(Request::new(
                shiroha_proto::shiroha_api::DeployFlowRequest {
                    flow_id: flow_id.to_string(),
                    wasm_bytes: wasm_for_manifest(manifest),
                },
            ))
            .await
            .expect("deploy flow");
    }

    async fn wait_for_job(
        service: &JobServiceImpl,
        job_id: &str,
        expected_state: &str,
        expected_current_state: &str,
    ) -> GetJobResponse {
        timeout(Duration::from_millis(400), async {
            loop {
                let job = service
                    .get_job(Request::new(GetJobRequest {
                        job_id: job_id.to_string(),
                    }))
                    .await
                    .expect("get job")
                    .into_inner();
                if job.state == expected_state && job.current_state == expected_current_state {
                    break job;
                }
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("job should reach expected state")
    }

    fn approval_manifest_to(flow_id: &str, terminal_state: &str) -> FlowManifest {
        FlowManifest {
            id: flow_id.to_string(),
            states: vec![
                StateDef {
                    name: "idle".into(),
                    kind: StateKind::Normal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: terminal_state.into(),
                    kind: StateKind::Terminal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
            ],
            transitions: vec![TransitionDef {
                from: "idle".into(),
                to: terminal_state.into(),
                event: "approve".into(),
                guard: None,
                action: Some("ship".into()),
                timeout: None,
            }],
            initial_state: "idle".into(),
            actions: vec![ActionDef {
                name: "ship".into(),
                dispatch: DispatchMode::Local,
            }],
        }
    }

    fn initial_on_enter_manifest(flow_id: &str) -> FlowManifest {
        FlowManifest {
            id: flow_id.to_string(),
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
            }],
        }
    }

    fn state_hooks_manifest(flow_id: &str) -> FlowManifest {
        FlowManifest {
            id: flow_id.to_string(),
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
                },
                ActionDef {
                    name: "exit".into(),
                    dispatch: DispatchMode::Local,
                },
            ],
        }
    }

    #[tokio::test]
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
        deploy_flow(
            harness.state.clone(),
            "approval",
            &approval_manifest_to("approval", "done"),
        )
        .await;

        let service = JobServiceImpl::new(harness.state.clone());
        let old_job = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: None,
            }))
            .await
            .expect("create old job")
            .into_inner();

        deploy_flow(
            harness.state.clone(),
            "approval",
            &approval_manifest_to("approval", "rerouted"),
        )
        .await;

        let new_job = service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "approval".into(),
                context: None,
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
