//! Redb 持久化存储后端
//!
//! 使用 [redb](https://docs.rs/redb) 嵌入式数据库实现 [`Storage`] trait。
//! 适用于单机生产部署场景。
//!
//! 数据以 JSON 序列化存储，表结构：
//! - `flows`: flow_id (str) → FlowRegistration (JSON bytes)
//! - `jobs`: job_id (16 bytes UUID) → Job (JSON bytes)
//! - `events`: job_id+event_id (32 bytes) → EventRecord (JSON bytes)

use std::path::Path;
use std::sync::Arc;

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use shiroha_core::error::{Result, ShirohaError};
use shiroha_core::event::EventRecord;
use shiroha_core::flow::FlowRegistration;
use shiroha_core::job::Job;
use shiroha_core::storage::Storage;
use uuid::Uuid;

const FLOWS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("flows");
const JOBS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("jobs");
const EVENTS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("events");

/// 将任意 Display 错误转为 ShirohaError::Storage
fn s(e: impl std::fmt::Display) -> ShirohaError {
    ShirohaError::Storage(e.to_string())
}

/// 基于 redb 的嵌入式存储
pub struct RedbStorage {
    db: Arc<Database>,
}

impl RedbStorage {
    /// 打开或创建数据库，自动建表
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = Database::create(path).map_err(s)?;
        // 首次启动时确保所有表存在
        let txn = db.begin_write().map_err(s)?;
        let _ = txn.open_table(FLOWS_TABLE).map_err(s)?;
        let _ = txn.open_table(JOBS_TABLE).map_err(s)?;
        let _ = txn.open_table(EVENTS_TABLE).map_err(s)?;
        txn.commit().map_err(s)?;
        Ok(Self { db: Arc::new(db) })
    }

    /// 事件表的复合键：job_id (16B) + event_id (16B) = 32B
    ///
    /// 这使得同一 Job 的事件在 B-tree 中连续排列，便于前缀扫描。
    fn event_key(job_id: Uuid, event_id: Uuid) -> [u8; 32] {
        let mut key = [0u8; 32];
        key[..16].copy_from_slice(job_id.as_bytes());
        key[16..].copy_from_slice(event_id.as_bytes());
        key
    }
}

impl Storage for RedbStorage {
    async fn save_flow(&self, flow: &FlowRegistration) -> Result<()> {
        let data = serde_json::to_vec(flow).map_err(s)?;
        let txn = self.db.begin_write().map_err(s)?;
        {
            let mut table = txn.open_table(FLOWS_TABLE).map_err(s)?;
            table
                .insert(flow.flow_id.as_str(), data.as_slice())
                .map_err(s)?;
        }
        txn.commit().map_err(s)?;
        Ok(())
    }

    async fn get_flow(&self, flow_id: &str) -> Result<Option<FlowRegistration>> {
        let txn = self.db.begin_read().map_err(s)?;
        let table = txn.open_table(FLOWS_TABLE).map_err(s)?;
        match table.get(flow_id).map_err(s)? {
            Some(data) => {
                let flow: FlowRegistration = serde_json::from_slice(data.value()).map_err(s)?;
                Ok(Some(flow))
            }
            None => Ok(None),
        }
    }

    async fn list_flows(&self) -> Result<Vec<FlowRegistration>> {
        let txn = self.db.begin_read().map_err(s)?;
        let table = txn.open_table(FLOWS_TABLE).map_err(s)?;
        let mut flows = Vec::new();
        for entry in table.iter().map_err(s)? {
            let (_, v) = entry.map_err(s)?;
            let flow: FlowRegistration = serde_json::from_slice(v.value()).map_err(s)?;
            flows.push(flow);
        }
        Ok(flows)
    }

    async fn delete_flow(&self, flow_id: &str) -> Result<()> {
        let txn = self.db.begin_write().map_err(s)?;
        {
            let mut table = txn.open_table(FLOWS_TABLE).map_err(s)?;
            table.remove(flow_id).map_err(s)?;
        }
        txn.commit().map_err(s)?;
        Ok(())
    }

    async fn save_job(&self, job: &Job) -> Result<()> {
        let data = serde_json::to_vec(job).map_err(s)?;
        let txn = self.db.begin_write().map_err(s)?;
        {
            let mut table = txn.open_table(JOBS_TABLE).map_err(s)?;
            table
                .insert(job.id.as_bytes().as_slice(), data.as_slice())
                .map_err(s)?;
        }
        txn.commit().map_err(s)?;
        Ok(())
    }

    async fn save_job_with_event(&self, job: &Job, event: &EventRecord) -> Result<()> {
        let job_data = serde_json::to_vec(job).map_err(s)?;
        let event_data = serde_json::to_vec(event).map_err(s)?;
        let event_key = Self::event_key(event.job_id, event.id);
        let txn = self.db.begin_write().map_err(s)?;
        // Job 快照与事件日志放进同一个 redb 写事务，避免只更新其中一份。
        {
            let mut jobs = txn.open_table(JOBS_TABLE).map_err(s)?;
            jobs.insert(job.id.as_bytes().as_slice(), job_data.as_slice())
                .map_err(s)?;
        }
        {
            let mut events = txn.open_table(EVENTS_TABLE).map_err(s)?;
            events
                .insert(event_key.as_slice(), event_data.as_slice())
                .map_err(s)?;
        }
        txn.commit().map_err(s)?;
        Ok(())
    }

    async fn get_job(&self, job_id: Uuid) -> Result<Option<Job>> {
        let txn = self.db.begin_read().map_err(s)?;
        let table = txn.open_table(JOBS_TABLE).map_err(s)?;
        match table.get(job_id.as_bytes().as_slice()).map_err(s)? {
            Some(data) => {
                let job: Job = serde_json::from_slice(data.value()).map_err(s)?;
                Ok(Some(job))
            }
            None => Ok(None),
        }
    }

    async fn list_jobs(&self, flow_id: &str) -> Result<Vec<Job>> {
        let txn = self.db.begin_read().map_err(s)?;
        let table = txn.open_table(JOBS_TABLE).map_err(s)?;
        let mut jobs = Vec::new();
        // 全表扫描，按 flow_id 过滤（小规模数据量下可接受）
        for entry in table.iter().map_err(s)? {
            let (_, v) = entry.map_err(s)?;
            let job: Job = serde_json::from_slice(v.value()).map_err(s)?;
            if job.flow_id == flow_id {
                jobs.push(job);
            }
        }
        Ok(jobs)
    }

    async fn append_event(&self, event: &EventRecord) -> Result<()> {
        let data = serde_json::to_vec(event).map_err(s)?;
        let key = Self::event_key(event.job_id, event.id);
        let txn = self.db.begin_write().map_err(s)?;
        {
            let mut table = txn.open_table(EVENTS_TABLE).map_err(s)?;
            table.insert(key.as_slice(), data.as_slice()).map_err(s)?;
        }
        txn.commit().map_err(s)?;
        Ok(())
    }

    async fn get_events(&self, job_id: Uuid) -> Result<Vec<EventRecord>> {
        let txn = self.db.begin_read().map_err(s)?;
        let table = txn.open_table(EVENTS_TABLE).map_err(s)?;
        let prefix = job_id.as_bytes();
        let mut events = Vec::new();
        // 当前实现没有额外的 job_id 二级索引，因此直接扫描 events 表，
        // 再利用复合键前 16 字节等于 job_id 的特性做过滤。
        for entry in table.iter().map_err(s)? {
            let (k, v) = entry.map_err(s)?;
            let key_bytes: &[u8] = k.value();
            if key_bytes.len() >= 16 && key_bytes[..16] == *prefix.as_slice() {
                let event: EventRecord = serde_json::from_slice(v.value()).map_err(s)?;
                events.push(event);
            }
        }
        // 对外统一按事件发生时间返回，避免上层依赖底层 B-tree 迭代顺序。
        events.sort_by_key(|e: &EventRecord| e.timestamp_ms);
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use shiroha_core::event::{EventKind, EventRecord};
    use shiroha_core::flow::{
        ActionDef, DispatchMode, FlowManifest, FlowRegistration, StateDef, StateKind, TransitionDef,
    };
    use shiroha_core::job::{Job, JobState};
    use shiroha_core::storage::Storage;

    use super::*;

    fn temp_db_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("shiroha-{name}-{}.redb", Uuid::now_v7()))
    }

    fn sample_flow_registration() -> FlowRegistration {
        FlowRegistration {
            flow_id: "demo".into(),
            version: Uuid::now_v7(),
            manifest: FlowManifest {
                id: "demo".into(),
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
                    event: "finish".into(),
                    guard: None,
                    action: Some("ship".into()),
                    timeout: None,
                }],
                initial_state: "idle".into(),
                actions: vec![ActionDef {
                    name: "ship".into(),
                    dispatch: DispatchMode::Local,
                }],
            },
            wasm_hash: "hash-demo".into(),
        }
    }

    fn sample_job(flow: &FlowRegistration) -> Job {
        Job {
            id: Uuid::now_v7(),
            flow_id: flow.flow_id.clone(),
            flow_version: flow.version,
            state: JobState::Running,
            current_state: flow.manifest.initial_state.clone(),
            context: Some(vec![1, 2, 3]),
        }
    }

    #[tokio::test]
    async fn persists_flow_job_and_event_across_reopen() {
        let path = temp_db_path("persist");
        let flow = sample_flow_registration();
        let job = sample_job(&flow);
        let event = EventRecord {
            id: Uuid::now_v7(),
            job_id: job.id,
            timestamp_ms: 100,
            kind: EventKind::Created {
                flow_id: flow.flow_id.clone(),
                flow_version: flow.version,
                initial_state: flow.manifest.initial_state.clone(),
            },
        };

        {
            let storage = RedbStorage::new(&path).expect("open db");
            storage.save_flow(&flow).await.expect("save flow");
            storage
                .save_job_with_event(&job, &event)
                .await
                .expect("save job with event");
        }

        let reopened = RedbStorage::new(&path).expect("reopen db");
        let stored_flow = reopened
            .get_flow(&flow.flow_id)
            .await
            .expect("get flow")
            .expect("flow exists");
        let stored_job = reopened
            .get_job(job.id)
            .await
            .expect("get job")
            .expect("job exists");
        let events = reopened.get_events(job.id).await.expect("events");
        let jobs = reopened.list_jobs(&flow.flow_id).await.expect("jobs");
        let flows = reopened.list_flows().await.expect("flows");

        assert_eq!(stored_flow.flow_id, flow.flow_id);
        assert_eq!(stored_flow.wasm_hash, "hash-demo");
        assert_eq!(stored_job.current_state, "idle");
        assert_eq!(stored_job.context, Some(vec![1, 2, 3]));
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].kind, EventKind::Created { .. }));
        assert_eq!(jobs.len(), 1);
        assert_eq!(flows.len(), 1);

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn get_events_returns_timestamp_sorted_records() {
        let path = temp_db_path("event-order");
        let flow = sample_flow_registration();
        let job = sample_job(&flow);
        let storage = RedbStorage::new(&path).expect("open db");

        storage.save_job(&job).await.expect("save job");
        storage
            .append_event(&EventRecord {
                id: Uuid::now_v7(),
                job_id: job.id,
                timestamp_ms: 200,
                kind: EventKind::Completed {
                    final_state: "done".into(),
                },
            })
            .await
            .expect("append newer event");
        storage
            .append_event(&EventRecord {
                id: Uuid::now_v7(),
                job_id: job.id,
                timestamp_ms: 100,
                kind: EventKind::Transition {
                    event: "finish".into(),
                    from: "idle".into(),
                    to: "done".into(),
                    action: Some("ship".into()),
                },
            })
            .await
            .expect("append older event");

        let events = storage.get_events(job.id).await.expect("events");

        assert_eq!(events.len(), 2);
        assert!(events[0].timestamp_ms < events[1].timestamp_ms);
        assert!(matches!(events[0].kind, EventKind::Transition { .. }));
        assert!(matches!(events[1].kind, EventKind::Completed { .. }));

        drop(storage);
        let _ = std::fs::remove_file(path);
    }
}
