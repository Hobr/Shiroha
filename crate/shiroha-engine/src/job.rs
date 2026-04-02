//! Job 生命周期管理
//!
//! [`JobManager`] 负责 Job 的创建、状态流转、事件溯源写入。
//! 所有状态变更同时追加事件记录，保证可审计。
//!
//! 并发控制由上层（shirohad）通过 Job 级别的 event inbox 保证串行化，
//! JobManager 本身不做并发控制。

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use shiroha_core::error::{Result, ShirohaError};
use shiroha_core::event::{EventKind, EventRecord};
use shiroha_core::job::{ExecutionStatus, Job, JobState};
use shiroha_core::storage::Storage;
use uuid::Uuid;

/// Job 生命周期管理器
///
/// 泛型参数 `S` 允许注入不同的存储后端（MemoryStorage / RedbStorage）。
pub struct JobManager<S: Storage> {
    storage: Arc<S>,
}

/// 当前时间戳（毫秒）
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 构造事件记录（自动生成 UUIDv7 和时间戳）
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

    /// 创建新 Job，初始状态为 Running
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
        let event = make_event(
            job.id,
            EventKind::Created {
                flow_id: flow_id.to_string(),
                flow_version,
                initial_state: initial_state.to_string(),
            },
        );
        self.storage.save_job_with_event(&job, &event).await?;
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

    /// 暂停 Job（Running → Paused）
    pub async fn pause_job(&self, job_id: Uuid) -> Result<()> {
        let mut job = self.load_job(job_id).await?;
        if job.state != JobState::Running {
            return Err(ShirohaError::InvalidJobState {
                expected: "running".into(),
                actual: job.state.to_string(),
            });
        }
        job.state = JobState::Paused;
        let event = make_event(job_id, EventKind::Paused);
        self.storage.save_job_with_event(&job, &event).await
    }

    /// 恢复 Job（Paused → Running）
    pub async fn resume_job(&self, job_id: Uuid) -> Result<()> {
        let mut job = self.load_job(job_id).await?;
        if job.state != JobState::Paused {
            return Err(ShirohaError::InvalidJobState {
                expected: "paused".into(),
                actual: job.state.to_string(),
            });
        }
        job.state = JobState::Running;
        let event = make_event(job_id, EventKind::Resumed);
        self.storage.save_job_with_event(&job, &event).await
    }

    /// 取消 Job（Running/Paused → Cancelled）
    pub async fn cancel_job(&self, job_id: Uuid) -> Result<()> {
        let mut job = self.load_job(job_id).await?;
        if job.state == JobState::Cancelled || job.state == JobState::Completed {
            return Err(ShirohaError::InvalidJobState {
                expected: "running or paused".into(),
                actual: job.state.to_string(),
            });
        }
        job.state = JobState::Cancelled;
        let event = make_event(job_id, EventKind::Cancelled);
        self.storage.save_job_with_event(&job, &event).await
    }

    /// 完成 Job（→ Completed）
    pub async fn complete_job(&self, job_id: Uuid, final_state: &str) -> Result<()> {
        let mut job = self.load_job(job_id).await?;
        job.state = JobState::Completed;
        job.current_state = final_state.to_string();
        let event = make_event(
            job_id,
            EventKind::Completed {
                final_state: final_state.to_string(),
            },
        );
        self.storage.save_job_with_event(&job, &event).await
    }

    /// 执行状态转移：更新 current_state 并记录 Transition 事件
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
        let event = make_event(
            job_id,
            EventKind::Transition {
                event: event.to_string(),
                from: from.to_string(),
                to: to.to_string(),
                action,
            },
        );
        self.storage.save_job_with_event(&job, &event).await
    }

    /// 记录 Action 执行完成事件，不修改 Job 状态。
    pub async fn record_action_result(
        &self,
        job_id: Uuid,
        action: &str,
        node_id: Option<String>,
        status: ExecutionStatus,
    ) -> Result<()> {
        self.storage
            .append_event(&make_event(
                job_id,
                EventKind::ActionComplete {
                    action: action.to_string(),
                    node_id,
                    status,
                },
            ))
            .await
    }

    /// 从存储加载 Job，不存在时返回 JobNotFound
    async fn load_job(&self, job_id: Uuid) -> Result<Job> {
        self.storage
            .get_job(job_id)
            .await?
            .ok_or_else(|| ShirohaError::JobNotFound(job_id.to_string()))
    }
}
