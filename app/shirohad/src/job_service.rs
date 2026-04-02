use std::sync::Arc;

use shiroha_proto::shiroha_api::job_service_server::JobService;
use shiroha_proto::shiroha_api::{
    CancelJobRequest, CancelJobResponse, CreateJobRequest, CreateJobResponse, GetJobEventsRequest,
    GetJobEventsResponse, GetJobRequest, GetJobResponse, ListJobsRequest, ListJobsResponse,
    PauseJobRequest, PauseJobResponse, ResumeJobRequest, ResumeJobResponse, TriggerEventRequest,
    TriggerEventResponse,
};
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::server::ShirohaState;

pub struct JobServiceImpl {
    state: Arc<ShirohaState>,
}

impl JobServiceImpl {
    pub fn new(state: Arc<ShirohaState>) -> Self {
        Self { state }
    }
}

fn parse_uuid(s: &str) -> Result<Uuid, Status> {
    s.parse::<Uuid>()
        .map_err(|_| Status::invalid_argument(format!("invalid UUID: {s}")))
}

#[tonic::async_trait]
impl JobService for JobServiceImpl {
    async fn create_job(
        &self,
        request: Request<CreateJobRequest>,
    ) -> Result<Response<CreateJobResponse>, Status> {
        let req = request.into_inner();
        let flows = self.state.flows.lock().await;
        let flow = flows
            .get(&req.flow_id)
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

        // Register timers for initial state transitions
        let initial_state = &flow.manifest.initial_state;
        for t in &flow.manifest.transitions {
            if t.from == *initial_state
                && let Some(ref timeout) = t.timeout
            {
                self.state.timer_wheel.register(
                    job.id,
                    timeout.timeout_event.clone(),
                    std::time::Duration::from_millis(timeout.duration_ms),
                );
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
        let job = self
            .state
            .job_manager
            .get_job(job_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("job not found"))?;

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

        let job = self
            .state
            .job_manager
            .get_job(job_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found("job not found"))?;

        let engines = self.state.engines.lock().await;
        let engine = engines
            .get(&job.flow_id)
            .ok_or_else(|| Status::internal("engine not found for flow"))?;

        let result = engine
            .process_event(&job.current_state, &req.event)
            .map_err(|e| Status::failed_precondition(e.to_string()))?;

        // Perform the transition
        self.state
            .job_manager
            .transition_job(job_id, &req.event, &result.from, &result.to, result.action)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        // Check if new state is terminal
        if engine.is_terminal(&result.to) {
            self.state
                .job_manager
                .complete_job(job_id, &result.to)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
            self.state.timer_wheel.cancel_all_job_timers(job_id).await;
        } else {
            // Register timers for new state transitions
            for t in &engine.manifest().transitions {
                if t.from == result.to
                    && let Some(ref timeout) = t.timeout
                {
                    self.state.timer_wheel.register(
                        job_id,
                        timeout.timeout_event.clone(),
                        std::time::Duration::from_millis(timeout.duration_ms),
                    );
                }
            }
        }

        tracing::info!(job_id = %job_id, event = req.event, from = result.from, to = result.to, "event triggered");
        Ok(Response::new(TriggerEventResponse {}))
    }

    async fn pause_job(
        &self,
        request: Request<PauseJobRequest>,
    ) -> Result<Response<PauseJobResponse>, Status> {
        let job_id = parse_uuid(&request.into_inner().job_id)?;
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
        self.state
            .job_manager
            .resume_job(job_id)
            .await
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        self.state.timer_wheel.resume_job_timers(job_id).await;
        Ok(Response::new(ResumeJobResponse {}))
    }

    async fn cancel_job(
        &self,
        request: Request<CancelJobRequest>,
    ) -> Result<Response<CancelJobResponse>, Status> {
        let job_id = parse_uuid(&request.into_inner().job_id)?;
        self.state
            .job_manager
            .cancel_job(job_id)
            .await
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        self.state.timer_wheel.cancel_all_job_timers(job_id).await;
        Ok(Response::new(CancelJobResponse {}))
    }

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
