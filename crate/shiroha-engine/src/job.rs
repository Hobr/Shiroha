use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use shiroha_core::error::{Result, ShirohaError};
use shiroha_core::event::{EventKind, EventRecord};
use shiroha_core::job::{Job, JobState};
use shiroha_core::storage::Storage;
use uuid::Uuid;

pub struct JobManager<S: Storage> {
    storage: Arc<S>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn make_event(job_id: Uuid, kind: EventKind) -> EventRecord {
    EventRecord {
        id: Uuid::now_v7(),
        job_id,
        timestamp_ms: now_ms(),
        kind,
    }
}

impl<S: Storage> JobManager<S> {
    pub fn new(storage: Arc<S>) -> Self {
        Self { storage }
    }

    pub async fn create_job(
        &self,
        flow_id: &str,
        flow_version: Uuid,
        initial_state: &str,
        context: Option<Vec<u8>>,
    ) -> Result<Job> {
        let job = Job {
            id: Uuid::now_v7(),
            flow_id: flow_id.to_string(),
            flow_version,
            state: JobState::Running,
            current_state: initial_state.to_string(),
            context,
        };
        self.storage.save_job(&job).await?;
        self.storage
            .append_event(&make_event(
                job.id,
                EventKind::Created {
                    flow_id: flow_id.to_string(),
                    flow_version,
                    initial_state: initial_state.to_string(),
                },
            ))
            .await?;
        Ok(job)
    }

    pub async fn get_job(&self, job_id: Uuid) -> Result<Option<Job>> {
        self.storage.get_job(job_id).await
    }

    pub async fn list_jobs(&self, flow_id: &str) -> Result<Vec<Job>> {
        self.storage.list_jobs(flow_id).await
    }

    pub async fn get_events(&self, job_id: Uuid) -> Result<Vec<EventRecord>> {
        self.storage.get_events(job_id).await
    }

    pub async fn pause_job(&self, job_id: Uuid) -> Result<()> {
        let mut job = self.load_job(job_id).await?;
        if job.state != JobState::Running {
            return Err(ShirohaError::InvalidJobState {
                expected: "running".into(),
                actual: job.state.to_string(),
            });
        }
        job.state = JobState::Paused;
        self.storage.save_job(&job).await?;
        self.storage.append_event(&make_event(job_id, EventKind::Paused)).await
    }

    pub async fn resume_job(&self, job_id: Uuid) -> Result<()> {
        let mut job = self.load_job(job_id).await?;
        if job.state != JobState::Paused {
            return Err(ShirohaError::InvalidJobState {
                expected: "paused".into(),
                actual: job.state.to_string(),
            });
        }
        job.state = JobState::Running;
        self.storage.save_job(&job).await?;
        self.storage.append_event(&make_event(job_id, EventKind::Resumed)).await
    }

    pub async fn cancel_job(&self, job_id: Uuid) -> Result<()> {
        let mut job = self.load_job(job_id).await?;
        if job.state == JobState::Cancelled || job.state == JobState::Completed {
            return Err(ShirohaError::InvalidJobState {
                expected: "running or paused".into(),
                actual: job.state.to_string(),
            });
        }
        job.state = JobState::Cancelled;
        self.storage.save_job(&job).await?;
        self.storage.append_event(&make_event(job_id, EventKind::Cancelled)).await
    }

    pub async fn complete_job(&self, job_id: Uuid, final_state: &str) -> Result<()> {
        let mut job = self.load_job(job_id).await?;
        job.state = JobState::Completed;
        job.current_state = final_state.to_string();
        self.storage.save_job(&job).await?;
        self.storage
            .append_event(&make_event(
                job_id,
                EventKind::Completed {
                    final_state: final_state.to_string(),
                },
            ))
            .await
    }

    pub async fn transition_job(
        &self,
        job_id: Uuid,
        event: &str,
        from: &str,
        to: &str,
        action: Option<String>,
    ) -> Result<()> {
        let mut job = self.load_job(job_id).await?;
        if job.state != JobState::Running {
            return Err(ShirohaError::InvalidJobState {
                expected: "running".into(),
                actual: job.state.to_string(),
            });
        }
        job.current_state = to.to_string();
        self.storage.save_job(&job).await?;
        self.storage
            .append_event(&make_event(
                job_id,
                EventKind::Transition {
                    event: event.to_string(),
                    from: from.to_string(),
                    to: to.to_string(),
                    action,
                },
            ))
            .await
    }

    async fn load_job(&self, job_id: Uuid) -> Result<Job> {
        self.storage
            .get_job(job_id)
            .await?
            .ok_or_else(|| ShirohaError::JobNotFound(job_id.to_string()))
    }
}
