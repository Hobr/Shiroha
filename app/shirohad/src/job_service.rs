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

#[cfg(test)]
mod tests {
    use tokio::time::{Duration, sleep, timeout};

    use super::*;
    use crate::flow_service::FlowServiceImpl;
    use crate::test_support::{
        TestHarness, approval_manifest, timeout_manifest, wasm_for_manifest,
    };
    use shiroha_core::event::EventKind;
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
        let job_uuid = job_id.parse::<Uuid>().expect("uuid");

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
        assert_eq!(
            harness
                .state
                .pending_events
                .lock()
                .await
                .get(&job_uuid)
                .map(std::collections::VecDeque::len),
            Some(1)
        );

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
        assert!(
            harness
                .state
                .pending_events
                .lock()
                .await
                .get(&job_uuid)
                .is_none()
        );

        let events = service
            .get_job_events(Request::new(GetJobEventsRequest {
                job_id: resumed.job_id.clone(),
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
            }))
            .await
            .expect("job events")
            .into_inner();
        assert_eq!(events.events.len(), 1);
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
}
