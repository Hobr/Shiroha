use shiroha_proto::shiroha_api::*;

use crate::client::ControlClient;
use crate::manifest::{manifest_event_names, manifest_state_names};

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

    pub async fn get_job(&mut self, job_id: &str) -> anyhow::Result<GetJobResponse> {
        Ok(self
            .job
            .get_job(GetJobRequest {
                job_id: job_id.to_string(),
            })
            .await?
            .into_inner())
    }

    pub async fn list_jobs_for_flow(
        &mut self,
        flow_id: &str,
    ) -> anyhow::Result<Vec<GetJobResponse>> {
        let mut jobs = self
            .job
            .list_jobs(ListJobsRequest {
                flow_id: flow_id.to_string(),
            })
            .await?
            .into_inner()
            .jobs;
        sort_jobs(&mut jobs);
        Ok(jobs)
    }

    pub async fn list_all_jobs(&mut self) -> anyhow::Result<Vec<GetJobResponse>> {
        let mut jobs = Vec::new();
        for flow_id in self.list_flow_ids().await? {
            jobs.extend(self.list_jobs_for_flow(&flow_id).await?);
        }
        sort_jobs(&mut jobs);
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
    ) -> anyhow::Result<Vec<EventRecord>> {
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
        Ok(events)
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
        let job = self.get_job(job_id).await?;
        let flow = self.get_flow(&job.flow_id, None).await?;
        Ok(manifest_event_names(&flow.manifest_json))
    }

    pub async fn list_wait_states(&mut self, job_id: &str) -> anyhow::Result<Vec<String>> {
        let job = self.get_job(job_id).await?;
        let flow = self.get_flow(&job.flow_id, None).await?;
        Ok(manifest_state_names(&flow.manifest_json))
    }
}

fn sort_jobs(jobs: &mut [GetJobResponse]) {
    jobs.sort_by(|left, right| {
        left.flow_id
            .cmp(&right.flow_id)
            .then_with(|| left.job_id.cmp(&right.job_id))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
