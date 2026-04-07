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
use shiroha_core::job::{ExecutionStatus, Job, JobState, PendingJobEvent, ScheduledTimeout};
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

fn freeze_timeout_schedule(job: &mut Job, timestamp_ms: u64) {
    let Some(anchor) = job.timeout_anchor_ms else {
        return;
    };
    let elapsed = timestamp_ms.saturating_sub(anchor);
    for timeout in &mut job.scheduled_timeouts {
        timeout.remaining_ms = timeout.remaining_ms.saturating_sub(elapsed);
    }
    job.timeout_anchor_ms = None;
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
        self.create_job_with_lifetime(flow_id, flow_version, initial_state, context, None, None)
            .await
    }

    pub async fn create_job_with_lifetime(
        &self,
        flow_id: &str,
        flow_version: Uuid,
        initial_state: &str,
        context: Option<Vec<u8>>,
        max_lifetime_ms: Option<u64>,
        lifetime_deadline_ms: Option<u64>,
    ) -> Result<Job> {
        let job = Job {
            id: Uuid::now_v7(),
            flow_id: flow_id.to_string(),
            flow_version,
            state: JobState::Running,
            current_state: initial_state.to_string(),
            context,
            pending_events: Vec::new(),
            scheduled_timeouts: Vec::new(),
            timeout_anchor_ms: None,
            max_lifetime_ms,
            lifetime_deadline_ms,
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

    pub async fn list_all_jobs(&self) -> Result<Vec<Job>> {
        self.storage.list_all_jobs().await
    }

    pub async fn get_events(&self, job_id: Uuid) -> Result<Vec<EventRecord>> {
        self.storage.get_events(job_id).await
    }

    pub async fn delete_job(&self, job_id: Uuid) -> Result<()> {
        let job = self.load_job(job_id).await?;
        if matches!(job.state, JobState::Running | JobState::Paused) {
            return Err(ShirohaError::InvalidJobState {
                expected: "cancelled or completed".into(),
                actual: job.state.to_string(),
            });
        }
        self.storage.delete_job(job_id).await
    }

    /// 向暂停中的 Job 追加待处理事件。
    pub async fn queue_pending_event(
        &self,
        job_id: Uuid,
        event: String,
        payload: Option<Vec<u8>>,
    ) -> Result<()> {
        let mut job = self.load_job(job_id).await?;
        if job.state != JobState::Paused {
            return Err(ShirohaError::InvalidJobState {
                expected: "paused".into(),
                actual: job.state.to_string(),
            });
        }
        job.pending_events.push(PendingJobEvent { event, payload });
        self.storage.save_job(&job).await
    }

    pub async fn pop_pending_event(&self, job_id: Uuid) -> Result<Option<PendingJobEvent>> {
        let mut job = self.load_job(job_id).await?;
        if job.pending_events.is_empty() {
            return Ok(None);
        }
        let next = job.pending_events.remove(0);
        self.storage.save_job(&job).await?;
        Ok(Some(next))
    }

    pub async fn push_front_pending_event(
        &self,
        job_id: Uuid,
        queued: PendingJobEvent,
    ) -> Result<()> {
        let mut job = self.load_job(job_id).await?;
        job.pending_events.insert(0, queued);
        self.storage.save_job(&job).await
    }

    pub async fn clear_pending_events(&self, job_id: Uuid) -> Result<()> {
        let mut job = self.load_job(job_id).await?;
        if job.pending_events.is_empty() {
            return Ok(());
        }
        job.pending_events.clear();
        self.storage.save_job(&job).await
    }

    pub async fn replace_timeout_schedule(
        &self,
        job_id: Uuid,
        timeouts: Vec<ScheduledTimeout>,
    ) -> Result<()> {
        let mut job = self.load_job(job_id).await?;
        job.scheduled_timeouts = timeouts;
        job.timeout_anchor_ms = (!job.scheduled_timeouts.is_empty()).then(now_ms);
        self.storage.save_job(&job).await
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
        freeze_timeout_schedule(&mut job, now_ms());
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
        job.timeout_anchor_ms = (!job.scheduled_timeouts.is_empty()).then(now_ms);
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
        job.pending_events.clear();
        job.scheduled_timeouts.clear();
        job.timeout_anchor_ms = None;
        let event = make_event(job_id, EventKind::Cancelled);
        self.storage.save_job_with_event(&job, &event).await
    }

    /// 完成 Job（→ Completed）
    pub async fn complete_job(&self, job_id: Uuid, final_state: &str) -> Result<()> {
        let mut job = self.load_job(job_id).await?;
        job.state = JobState::Completed;
        job.current_state = final_state.to_string();
        job.pending_events.clear();
        job.scheduled_timeouts.clear();
        job.timeout_anchor_ms = None;
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
        job.scheduled_timeouts.clear();
        job.timeout_anchor_ms = None;
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

#[cfg(test)]
mod tests {
    use shiroha_core::event::EventKind;
    use shiroha_core::job::ExecutionStatus;
    use shiroha_core::storage::{MemoryStorage, Storage};

    use super::*;

    #[tokio::test]
    async fn create_job_persists_job_and_created_event() {
        let storage = Arc::new(MemoryStorage::new());
        let manager = JobManager::new(storage.clone());
        let flow_version = Uuid::now_v7();

        let job = manager
            .create_job("demo", flow_version, "idle", Some(vec![1, 2, 3]))
            .await
            .expect("job created");

        let stored = storage
            .get_job(job.id)
            .await
            .expect("read job")
            .expect("job exists");
        assert_eq!(stored.flow_id, "demo");
        assert_eq!(stored.current_state, "idle");
        assert_eq!(stored.state, JobState::Running);
        assert_eq!(stored.context, Some(vec![1, 2, 3]));
        assert!(stored.pending_events.is_empty());
        assert!(stored.scheduled_timeouts.is_empty());
        assert!(stored.timeout_anchor_ms.is_none());

        let events = manager.get_events(job.id).await.expect("events");
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0].kind,
            EventKind::Created {
                flow_id,
                flow_version: version,
                initial_state,
            } if flow_id == "demo" && *version == flow_version && initial_state == "idle"
        ));
    }

    #[tokio::test]
    async fn lifecycle_transitions_and_action_results_are_recorded() {
        let storage = Arc::new(MemoryStorage::new());
        let manager = JobManager::new(storage);
        let job = manager
            .create_job("demo", Uuid::now_v7(), "idle", None)
            .await
            .expect("job created");

        manager.pause_job(job.id).await.expect("pause");
        manager.resume_job(job.id).await.expect("resume");
        manager
            .transition_job(job.id, "finish", "idle", "done", Some("ship".into()))
            .await
            .expect("transition");
        manager
            .record_action_result(
                job.id,
                "ship",
                Some("standalone".into()),
                ExecutionStatus::Success,
            )
            .await
            .expect("action result");
        manager
            .complete_job(job.id, "done")
            .await
            .expect("complete");

        let final_job = manager
            .get_job(job.id)
            .await
            .expect("read job")
            .expect("job exists");
        assert_eq!(final_job.state, JobState::Completed);
        assert_eq!(final_job.current_state, "done");

        let events = manager.get_events(job.id).await.expect("events");
        assert_eq!(events.len(), 6);
        assert!(matches!(events[0].kind, EventKind::Created { .. }));
        assert!(matches!(events[1].kind, EventKind::Paused));
        assert!(matches!(events[2].kind, EventKind::Resumed));
        assert!(matches!(
            &events[3].kind,
            EventKind::Transition {
                event,
                from,
                to,
                action,
            } if event == "finish" && from == "idle" && to == "done" && action.as_deref() == Some("ship")
        ));
        assert!(matches!(
            &events[4].kind,
            EventKind::ActionComplete {
                action,
                node_id,
                status,
            } if action == "ship" && node_id.as_deref() == Some("standalone") && *status == ExecutionStatus::Success
        ));
        assert!(matches!(
            &events[5].kind,
            EventKind::Completed { final_state } if final_state == "done"
        ));
    }

    #[tokio::test]
    async fn invalid_lifecycle_transition_returns_error() {
        let storage = Arc::new(MemoryStorage::new());
        let manager = JobManager::new(storage);
        let job = manager
            .create_job("demo", Uuid::now_v7(), "idle", None)
            .await
            .expect("job created");

        manager.pause_job(job.id).await.expect("pause");
        let error = manager
            .pause_job(job.id)
            .await
            .expect_err("second pause fails");

        match error {
            ShirohaError::InvalidJobState { expected, actual } => {
                assert_eq!(expected, "running");
                assert_eq!(actual, "paused");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[tokio::test]
    async fn delete_job_requires_terminal_state() {
        let storage = Arc::new(MemoryStorage::new());
        let manager = JobManager::new(storage.clone());
        let job = manager
            .create_job("demo", Uuid::now_v7(), "idle", None)
            .await
            .expect("job created");

        let error = manager.delete_job(job.id).await.expect_err("running job");
        assert!(matches!(error, ShirohaError::InvalidJobState { .. }));

        manager.cancel_job(job.id).await.expect("cancel");
        manager.delete_job(job.id).await.expect("delete");

        assert!(storage.get_job(job.id).await.expect("get job").is_none());
        assert!(
            storage
                .get_events(job.id)
                .await
                .expect("get events")
                .is_empty()
        );
    }

    #[tokio::test]
    async fn pending_events_are_persisted_with_job_snapshot() {
        let storage = Arc::new(MemoryStorage::new());
        let manager = JobManager::new(storage.clone());
        let job = manager
            .create_job("demo", Uuid::now_v7(), "idle", None)
            .await
            .expect("job created");

        manager.pause_job(job.id).await.expect("pause");
        manager
            .queue_pending_event(job.id, "approve".into(), Some(vec![1, 2]))
            .await
            .expect("queue event");

        let stored = storage
            .get_job(job.id)
            .await
            .expect("get job")
            .expect("job exists");
        assert_eq!(stored.pending_events.len(), 1);
        assert_eq!(stored.pending_events[0].event, "approve");
        assert_eq!(
            stored.pending_events[0].payload.as_deref(),
            Some(&[1, 2][..])
        );

        let popped = manager
            .pop_pending_event(job.id)
            .await
            .expect("pop event")
            .expect("event exists");
        assert_eq!(popped.event, "approve");

        let stored = storage
            .get_job(job.id)
            .await
            .expect("get job")
            .expect("job exists");
        assert!(stored.pending_events.is_empty());
    }

    #[tokio::test]
    async fn timeout_schedule_is_frozen_on_pause_and_rearmed_on_resume() {
        let storage = Arc::new(MemoryStorage::new());
        let manager = JobManager::new(storage.clone());
        let job = manager
            .create_job("demo", Uuid::now_v7(), "idle", None)
            .await
            .expect("job created");

        manager
            .replace_timeout_schedule(
                job.id,
                vec![ScheduledTimeout {
                    event: "expire".into(),
                    remaining_ms: 200,
                }],
            )
            .await
            .expect("set timeout schedule");

        let armed = storage
            .get_job(job.id)
            .await
            .expect("get job")
            .expect("job exists");
        assert!(armed.timeout_anchor_ms.is_some());
        assert_eq!(armed.scheduled_timeouts[0].remaining_ms, 200);

        manager.pause_job(job.id).await.expect("pause");
        let paused = storage
            .get_job(job.id)
            .await
            .expect("get job")
            .expect("job exists");
        assert_eq!(paused.state, JobState::Paused);
        assert!(paused.timeout_anchor_ms.is_none());
        assert!(paused.scheduled_timeouts[0].remaining_ms <= 200);

        manager.resume_job(job.id).await.expect("resume");
        let resumed = storage
            .get_job(job.id)
            .await
            .expect("get job")
            .expect("job exists");
        assert_eq!(resumed.state, JobState::Running);
        assert!(resumed.timeout_anchor_ms.is_some());
    }

    #[tokio::test]
    async fn create_job_with_lifetime_persists_lifetime_fields() {
        let storage = Arc::new(MemoryStorage::new());
        let manager = JobManager::new(storage.clone());
        let flow_version = Uuid::now_v7();
        let deadline_ms = 9_999_999_999_u64;

        let job = manager
            .create_job_with_lifetime(
                "demo",
                flow_version,
                "idle",
                None,
                Some(500),
                Some(deadline_ms),
            )
            .await
            .expect("job created with lifetime");

        assert_eq!(job.max_lifetime_ms, Some(500));
        assert_eq!(job.lifetime_deadline_ms, Some(deadline_ms));

        // Verify the values survive a storage round-trip (simulates controller restart).
        let stored = storage
            .get_job(job.id)
            .await
            .expect("read job")
            .expect("job exists");
        assert_eq!(stored.max_lifetime_ms, Some(500));
        assert_eq!(stored.lifetime_deadline_ms, Some(deadline_ms));
    }

    #[tokio::test]
    async fn lifetime_deadline_is_preserved_across_pause_and_resume() {
        // Verifies that pause/resume does NOT touch lifetime_deadline_ms, so that
        // the wall-clock deadline keeps ticking even while the job is paused.
        let storage = Arc::new(MemoryStorage::new());
        let manager = JobManager::new(storage.clone());
        let deadline_ms = 9_999_999_999_u64;

        let job = manager
            .create_job_with_lifetime(
                "demo",
                Uuid::now_v7(),
                "idle",
                None,
                Some(1_000),
                Some(deadline_ms),
            )
            .await
            .expect("job created");

        manager.pause_job(job.id).await.expect("pause");
        let paused = storage
            .get_job(job.id)
            .await
            .expect("get job")
            .expect("job exists");
        assert_eq!(
            paused.lifetime_deadline_ms,
            Some(deadline_ms),
            "pause must not modify lifetime_deadline_ms"
        );

        manager.resume_job(job.id).await.expect("resume");
        let resumed = storage
            .get_job(job.id)
            .await
            .expect("get job")
            .expect("job exists");
        assert_eq!(
            resumed.lifetime_deadline_ms,
            Some(deadline_ms),
            "resume must not modify lifetime_deadline_ms"
        );
    }
}
