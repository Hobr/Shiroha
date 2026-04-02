//! gRPC JobService 实现
//!
//! 处理 Job 的创建、状态查询、事件触发、生命周期管理（暂停/恢复/取消）。
//! Phase 1 里所有事件都按 Job 串行处理，暂停期间事件会暂存在内存队列中。

use std::sync::Arc;

use shiroha_core::error::ShirohaError;
use shiroha_core::flow::{DispatchMode, FlowRegistration, TimeoutDef};
use shiroha_core::job::{ActionResult, ExecutionStatus, Job, JobState};
use shiroha_proto::shiroha_api::job_service_server::JobService;
use shiroha_proto::shiroha_api::{
    CancelJobRequest, CancelJobResponse, CreateJobRequest, CreateJobResponse, GetJobEventsRequest,
    GetJobEventsResponse, GetJobRequest, GetJobResponse, ListJobsRequest, ListJobsResponse,
    PauseJobRequest, PauseJobResponse, ResumeJobRequest, ResumeJobResponse, TriggerEventRequest,
    TriggerEventResponse,
};
use shiroha_wasm::host::{ActionContext, GuardContext, WasmHost};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::server::ShirohaState;

pub(crate) struct QueuedJobEvent {
    pub event: String,
    pub payload: Option<Vec<u8>>,
}

pub struct JobServiceImpl {
    state: Arc<ShirohaState>,
}

impl JobServiceImpl {
    pub fn new(state: Arc<ShirohaState>) -> Self {
        Self { state }
    }

    pub async fn enqueue_event(
        &self,
        job_id: Uuid,
        event: String,
        payload: Option<Vec<u8>>,
    ) -> Result<(), Status> {
        let lock = self.job_lock(job_id).await;
        let _guard = lock.lock().await;
        self.handle_event_locked(job_id, event, payload).await
    }

    async fn job_lock(&self, job_id: Uuid) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self.state.job_locks.lock().await;
        locks
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

    async fn queue_event(&self, job_id: Uuid, event: String, payload: Option<Vec<u8>>) {
        self.state
            .pending_events
            .lock()
            .await
            .entry(job_id)
            .or_default()
            .push_back(QueuedJobEvent { event, payload });
    }

    async fn pop_queued_event(&self, job_id: Uuid) -> Option<QueuedJobEvent> {
        let mut queues = self.state.pending_events.lock().await;
        let next = queues.get_mut(&job_id).and_then(|queue| queue.pop_front());
        if queues.get(&job_id).is_some_and(|queue| queue.is_empty()) {
            queues.remove(&job_id);
        }
        next
    }

    async fn push_front_queued_event(&self, job_id: Uuid, queued: QueuedJobEvent) {
        self.state
            .pending_events
            .lock()
            .await
            .entry(job_id)
            .or_default()
            .push_front(queued);
    }

    async fn clear_queued_events(&self, job_id: Uuid) {
        self.state.pending_events.lock().await.remove(&job_id);
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
                self.queue_event(job_id, event.clone(), payload).await;
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
        let flow = self.flow_registration(&job.flow_id).await?;
        let (from, to, action, guard, is_terminal, timeouts) = {
            let engines = self.state.engines.lock().await;
            let engine = engines
                .get(&job.flow_id)
                .ok_or_else(|| Status::internal("engine not found for flow"))?;
            let result = engine
                .process_event(&job.current_state, &event)
                .map_err(|e| Status::failed_precondition(e.to_string()))?;
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
                engine.is_terminal(&result.to),
                next_timeouts,
            )
        };

        if let Some(guard_name) = guard.as_deref() {
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

        self.state.timer_wheel.cancel_all_job_timers(job.id).await;
        self.state
            .job_manager
            .transition_job(job.id, &event, &from, &to, action.clone())
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let mut action_failure = None;
        if let Some(action_name) = action.as_deref() {
            let action_result = self
                .invoke_action(&flow, action_name, job.id, &to, payload.clone())
                .await?;
            self.record_action_result(job.id, action_name, &action_result)
                .await?;
            if action_result.status != ExecutionStatus::Success {
                action_failure = Some(format!(
                    "transition committed but action `{action_name}` finished with status {}",
                    execution_status_name(action_result.status)
                ));
            }
        }

        if is_terminal {
            self.state
                .job_manager
                .complete_job(job.id, &to)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
            self.state.timer_wheel.cancel_all_job_timers(job.id).await;
        } else {
            for timeout in timeouts {
                self.register_timeout(job.id, timeout);
            }
        }

        tracing::info!(job_id = %job.id, event, from, to, "event processed");

        if let Some(message) = action_failure {
            return Err(Status::aborted(message));
        }

        Ok(())
    }

    async fn drain_pending_events_locked(&self, job_id: Uuid) -> Result<(), Status> {
        while let Some(queued) = self.pop_queued_event(job_id).await {
            let job = self.load_job(job_id).await?;
            match job.state {
                JobState::Running => {
                    self.process_running_event(job, queued.event, queued.payload)
                        .await?;
                }
                JobState::Paused => {
                    self.push_front_queued_event(job_id, queued).await;
                    return Ok(());
                }
                JobState::Cancelled | JobState::Completed => {
                    self.clear_queued_events(job_id).await;
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

    fn register_timeout(&self, job_id: Uuid, timeout: TimeoutDef) {
        self.state.timer_wheel.register(
            job_id,
            timeout.timeout_event,
            std::time::Duration::from_millis(timeout.duration_ms),
        );
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

        WasmHost::new(self.state.wasm_runtime.engine(), module.module())
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

#[tonic::async_trait]
impl JobService for JobServiceImpl {
    /// 创建 Job：查找 Flow → 创建运行实例 → 注册初始状态的定时器
    async fn create_job(
        &self,
        request: Request<CreateJobRequest>,
    ) -> Result<Response<CreateJobResponse>, Status> {
        let req = request.into_inner();
        let flow = self
            .state
            .flows
            .lock()
            .await
            .get(&req.flow_id)
            .cloned()
            .ok_or_else(|| Status::not_found(format!("flow `{}` not found", req.flow_id)))?;

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

        for transition in &flow.manifest.transitions {
            if transition.from == flow.manifest.initial_state
                && let Some(timeout) = transition.timeout.clone()
            {
                self.register_timeout(job.id, timeout);
            }
        }

        tracing::info!(job_id = %job.id, flow_id = req.flow_id, "job created");
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

        Ok(Response::new(GetJobResponse {
            job_id: job.id.to_string(),
            flow_id: job.flow_id,
            state: job.state.to_string(),
            current_state: job.current_state,
        }))
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

        let resp = jobs
            .iter()
            .map(|j| GetJobResponse {
                job_id: j.id.to_string(),
                flow_id: j.flow_id.clone(),
                state: j.state.to_string(),
                current_state: j.current_state.clone(),
            })
            .collect();

        Ok(Response::new(ListJobsResponse { jobs: resp }))
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
        self.clear_queued_events(job_id).await;

        Ok(Response::new(CancelJobResponse {}))
    }

    /// 查询 Job 的事件溯源日志
    async fn get_job_events(
        &self,
        request: Request<GetJobEventsRequest>,
    ) -> Result<Response<GetJobEventsResponse>, Status> {
        let job_id = parse_uuid(&request.into_inner().job_id)?;
        let events = self
            .state
            .job_manager
            .get_events(job_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

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
