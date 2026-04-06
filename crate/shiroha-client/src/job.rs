use anyhow::Context;
use serde_json::Value;
use shiroha_proto::shiroha_api::*;

use crate::client::ControlClient;
use crate::manifest::{manifest_event_names, manifest_state_names, parse_json_value_required};

#[derive(Debug, Clone, Default)]
pub struct EventQuery {
    pub since_id: Option<String>,
    pub since_timestamp_ms: Option<u64>,
    pub limit: Option<u32>,
    pub kind_filters: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForceDeleteJobResult {
    pub job_id: String,
    pub previous_state: String,
    pub cancelled_before_delete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobDetails {
    pub job_id: String,
    pub flow_id: String,
    pub state: String,
    pub current_state: String,
    pub flow_version: String,
    pub context_bytes: Option<u64>,
}

impl From<GetJobResponse> for JobDetails {
    fn from(value: GetJobResponse) -> Self {
        Self {
            job_id: value.job_id,
            flow_id: value.flow_id,
            state: value.state,
            current_state: value.current_state,
            flow_version: value.flow_version,
            context_bytes: value.context_bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobEvent {
    pub id: String,
    pub job_id: String,
    pub timestamp_ms: u64,
    pub kind: Value,
}

impl TryFrom<EventRecord> for JobEvent {
    type Error = anyhow::Error;

    fn try_from(value: EventRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            job_id: value.job_id,
            timestamp_ms: value.timestamp_ms,
            kind: parse_json_value_required(&value.kind_json, "kind_json")
                .context("invalid job event payload returned by server")?,
        })
    }
}

impl ControlClient {
    pub async fn create_job(
        &mut self,
        flow_id: &str,
        context: Option<Vec<u8>>,
    ) -> anyhow::Result<CreateJobResponse> {
        Ok(self
            .job
            .create_job(CreateJobRequest {
                flow_id: flow_id.to_string(),
                context,
            })
            .await?
            .into_inner())
    }

    pub async fn get_job(&mut self, job_id: &str) -> anyhow::Result<JobDetails> {
        Ok(self
            .job
            .get_job(GetJobRequest {
                job_id: job_id.to_string(),
            })
            .await?
            .into_inner()
            .into())
    }

    pub async fn list_jobs_for_flow(&mut self, flow_id: &str) -> anyhow::Result<Vec<JobDetails>> {
        let mut jobs = self
            .job
            .list_jobs(ListJobsRequest {
                flow_id: flow_id.to_string(),
            })
            .await?
            .into_inner()
            .jobs;
        sort_jobs(&mut jobs);
        Ok(jobs.into_iter().map(JobDetails::from).collect())
    }

    pub async fn list_all_jobs(&mut self) -> anyhow::Result<Vec<JobDetails>> {
        let mut jobs = Vec::new();
        for flow_id in self.list_flow_ids().await? {
            jobs.extend(self.list_jobs_for_flow(&flow_id).await?);
        }
        sort_job_details(&mut jobs);
        Ok(jobs)
    }

    pub async fn list_job_ids(&mut self) -> anyhow::Result<Vec<String>> {
        let mut job_ids = self
            .list_all_jobs()
            .await?
            .into_iter()
            .map(|job| job.job_id)
            .collect::<Vec<_>>();
        job_ids.sort_unstable();
        job_ids.dedup();
        Ok(job_ids)
    }

    pub async fn delete_job(&mut self, job_id: &str) -> anyhow::Result<DeleteJobResponse> {
        Ok(self
            .job
            .delete_job(DeleteJobRequest {
                job_id: job_id.to_string(),
            })
            .await?
            .into_inner())
    }

    pub async fn force_delete_job(&mut self, job_id: &str) -> anyhow::Result<ForceDeleteJobResult> {
        let job = self.get_job(job_id).await?;
        let cancelled_before_delete = matches!(job.state.as_str(), "running" | "paused");
        if cancelled_before_delete {
            self.cancel_job(job_id).await?;
        }
        self.delete_job(job_id).await?;
        Ok(ForceDeleteJobResult {
            job_id: job.job_id,
            previous_state: job.state,
            cancelled_before_delete,
        })
    }

    pub async fn trigger_event(
        &mut self,
        job_id: &str,
        event: &str,
        payload: Option<Vec<u8>>,
    ) -> anyhow::Result<()> {
        self.job
            .trigger_event(TriggerEventRequest {
                job_id: job_id.to_string(),
                event: event.to_string(),
                payload,
            })
            .await?;
        Ok(())
    }

    pub async fn pause_job(&mut self, job_id: &str) -> anyhow::Result<()> {
        self.job
            .pause_job(PauseJobRequest {
                job_id: job_id.to_string(),
            })
            .await?;
        Ok(())
    }

    pub async fn resume_job(&mut self, job_id: &str) -> anyhow::Result<()> {
        self.job
            .resume_job(ResumeJobRequest {
                job_id: job_id.to_string(),
            })
            .await?;
        Ok(())
    }

    pub async fn cancel_job(&mut self, job_id: &str) -> anyhow::Result<()> {
        self.job
            .cancel_job(CancelJobRequest {
                job_id: job_id.to_string(),
            })
            .await?;
        Ok(())
    }

    pub async fn get_job_events(
        &mut self,
        job_id: &str,
        query: &EventQuery,
    ) -> anyhow::Result<Vec<JobEvent>> {
        let mut events = self
            .job
            .get_job_events(GetJobEventsRequest {
                job_id: job_id.to_string(),
                since_id: query.since_id.clone(),
                since_timestamp_ms: query.since_timestamp_ms,
                limit: query.limit,
                kind: query.kind_filters.clone(),
            })
            .await?
            .into_inner()
            .events;
        events.sort_by(|left, right| {
            left.timestamp_ms
                .cmp(&right.timestamp_ms)
                .then_with(|| left.id.cmp(&right.id))
        });
        events.into_iter().map(JobEvent::try_from).collect()
    }

    pub async fn list_job_event_ids(&mut self, job_id: &str) -> anyhow::Result<Vec<String>> {
        Ok(self
            .get_job_events(job_id, &EventQuery::default())
            .await?
            .into_iter()
            .map(|event| event.id)
            .collect())
    }

    pub async fn list_job_event_names(&mut self, job_id: &str) -> anyhow::Result<Vec<String>> {
        let flow = self.get_bound_flow_for_job(job_id).await?;
        Ok(manifest_event_names(&flow.manifest))
    }

    pub async fn list_wait_states(&mut self, job_id: &str) -> anyhow::Result<Vec<String>> {
        let flow = self.get_bound_flow_for_job(job_id).await?;
        Ok(manifest_state_names(&flow.manifest))
    }

    async fn get_bound_flow_for_job(
        &mut self,
        job_id: &str,
    ) -> anyhow::Result<crate::flow::FlowDetails> {
        let job = self.get_job(job_id).await?;
        let request = bound_flow_request(&job);
        self.get_flow(&request.flow_id, request.version.as_deref())
            .await
    }
}

fn bound_flow_request(job: &JobDetails) -> GetFlowRequest {
    GetFlowRequest {
        flow_id: job.flow_id.clone(),
        version: Some(job.flow_version.clone()),
    }
}

fn sort_jobs(jobs: &mut [GetJobResponse]) {
    jobs.sort_by(|left, right| {
        left.flow_id
            .cmp(&right.flow_id)
            .then_with(|| left.job_id.cmp(&right.job_id))
    });
}

fn sort_job_details(jobs: &mut [JobDetails]) {
    jobs.sort_by(|left, right| {
        left.flow_id
            .cmp(&right.flow_id)
            .then_with(|| left.job_id.cmp(&right.job_id))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn force_delete_result_carries_state_information() {
        let result = ForceDeleteJobResult {
            job_id: "job-1".into(),
            previous_state: "running".into(),
            cancelled_before_delete: true,
        };

        assert_eq!(result.job_id, "job-1");
        assert_eq!(result.previous_state, "running");
        assert!(result.cancelled_before_delete);
    }

    #[test]
    fn job_details_extract_proto_fields() {
        let details = JobDetails::from(GetJobResponse {
            job_id: "job-1".into(),
            flow_id: "flow-a".into(),
            state: "running".into(),
            current_state: "review".into(),
            flow_version: "v1".into(),
            context_bytes: Some(42),
        });

        assert_eq!(details.job_id, "job-1");
        assert_eq!(details.flow_id, "flow-a");
        assert_eq!(details.state, "running");
        assert_eq!(details.current_state, "review");
        assert_eq!(details.flow_version, "v1");
        assert_eq!(details.context_bytes, Some(42));
    }

    #[test]
    fn job_event_parses_kind_json() {
        let event = JobEvent::try_from(EventRecord {
            id: "event-1".into(),
            job_id: "job-1".into(),
            timestamp_ms: 123,
            kind_json: r#"{"type":"created"}"#.into(),
        })
        .expect("kind json should parse");

        assert_eq!(event.id, "event-1");
        assert_eq!(event.kind, json!({"type": "created"}));
    }

    #[test]
    fn bound_flow_request_uses_job_bound_version() {
        let request = bound_flow_request(&JobDetails {
            job_id: "job-1".into(),
            flow_id: "flow-a".into(),
            state: "running".into(),
            current_state: "draft".into(),
            flow_version: "bound-version".into(),
            context_bytes: None,
        });

        assert_eq!(request.flow_id, "flow-a");
        assert_eq!(request.version.as_deref(), Some("bound-version"));
    }
}
