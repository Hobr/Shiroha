//! Redb 持久化存储后端
//!
//! 使用 [redb](https://docs.rs/redb) 嵌入式数据库实现 [`Storage`] trait。
//! 适用于单机生产部署场景。
//!
//! 数据以 JSON 序列化存储，表结构：
//! - `flows`: flow_id (str) → 最新 FlowRegistration (JSON bytes)
//! - `flow_versions`: hex(flow_id bytes) + `\0` + version → 指定版本 FlowRegistration (JSON bytes)
//! - `wasm_modules`: wasm_hash (str) → 原始 wasm bytes
//! - `jobs`: job_id (16 bytes UUID) → Job (JSON bytes)
//! - `events`: job_id+event_id (32 bytes) → EventRecord (JSON bytes)

use std::path::Path;
use std::sync::Arc;

use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use shiroha_core::error::{Result, ShirohaError};
use shiroha_core::event::EventRecord;
use shiroha_core::flow::FlowRegistration;
use shiroha_core::job::Job;
use shiroha_core::storage::{CapabilityStore, Storage};
use uuid::Uuid;

const FLOWS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("flows");
const FLOW_VERSIONS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("flow_versions");
const WASM_MODULES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("wasm_modules");
const JOBS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("jobs");
const EVENTS_TABLE: TableDefinition<&[u8], &[u8]> = TableDefinition::new("events");
const KV_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("kv");

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
        let _ = txn.open_table(FLOW_VERSIONS_TABLE).map_err(s)?;
        let _ = txn.open_table(WASM_MODULES_TABLE).map_err(s)?;
        let _ = txn.open_table(JOBS_TABLE).map_err(s)?;
        let _ = txn.open_table(EVENTS_TABLE).map_err(s)?;
        let _ = txn.open_table(KV_TABLE).map_err(s)?;
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

    fn flow_version_key(flow_id: &str, version: Uuid) -> String {
        format!("{}\u{0}{version}", Self::encode_flow_id(flow_id))
    }

    fn flow_version_prefix_bounds(flow_id: &str) -> (String, String) {
        let encoded_flow_id = Self::encode_flow_id(flow_id);
        (
            format!("{encoded_flow_id}\u{0}"),
            format!("{encoded_flow_id}\u{1}"),
        )
    }

    fn encode_flow_id(flow_id: &str) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut encoded = String::with_capacity(flow_id.len() * 2);
        for byte in flow_id.as_bytes() {
            encoded.push(HEX[(byte >> 4) as usize] as char);
            encoded.push(HEX[(byte & 0x0f) as usize] as char);
        }
        encoded
    }

    fn event_key_range(job_id: Uuid) -> ([u8; 32], [u8; 32]) {
        let mut start = [0u8; 32];
        let mut end = [0xFFu8; 32];
        start[..16].copy_from_slice(job_id.as_bytes());
        end[..16].copy_from_slice(job_id.as_bytes());
        (start, end)
    }

    fn kv_key(namespace: &str, key: &str) -> String {
        format!("{namespace}\u{0}{key}")
    }
}

impl Storage for RedbStorage {
    async fn save_flow(&self, flow: &FlowRegistration) -> Result<()> {
        let data = serde_json::to_vec(flow).map_err(s)?;
        let version_key = Self::flow_version_key(&flow.flow_id, flow.version);
        let txn = self.db.begin_write().map_err(s)?;
        {
            let mut table = txn.open_table(FLOWS_TABLE).map_err(s)?;
            table
                .insert(flow.flow_id.as_str(), data.as_slice())
                .map_err(s)?;
        }
        {
            let mut table = txn.open_table(FLOW_VERSIONS_TABLE).map_err(s)?;
            table
                .insert(version_key.as_str(), data.as_slice())
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

    async fn get_flow_version(
        &self,
        flow_id: &str,
        version: Uuid,
    ) -> Result<Option<FlowRegistration>> {
        let txn = self.db.begin_read().map_err(s)?;
        let table = txn.open_table(FLOW_VERSIONS_TABLE).map_err(s)?;
        let version_key = Self::flow_version_key(flow_id, version);
        match table.get(version_key.as_str()).map_err(s)? {
            Some(data) => {
                let flow: FlowRegistration = serde_json::from_slice(data.value()).map_err(s)?;
                Ok(Some(flow))
            }
            None => Ok(None),
        }
    }

    async fn list_flow_versions(&self) -> Result<Vec<FlowRegistration>> {
        let txn = self.db.begin_read().map_err(s)?;
        let table = txn.open_table(FLOW_VERSIONS_TABLE).map_err(s)?;
        let mut flows = Vec::new();
        for entry in table.iter().map_err(s)? {
            let (_, v) = entry.map_err(s)?;
            let flow: FlowRegistration = serde_json::from_slice(v.value()).map_err(s)?;
            flows.push(flow);
        }
        Ok(flows)
    }

    async fn list_flow_versions_for(&self, flow_id: &str) -> Result<Vec<FlowRegistration>> {
        let txn = self.db.begin_read().map_err(s)?;
        let table = txn.open_table(FLOW_VERSIONS_TABLE).map_err(s)?;
        let (start, end) = Self::flow_version_prefix_bounds(flow_id);
        let mut flows = Vec::new();
        for entry in table.range(start.as_str()..end.as_str()).map_err(s)? {
            let (_, v) = entry.map_err(s)?;
            let flow: FlowRegistration = serde_json::from_slice(v.value()).map_err(s)?;
            flows.push(flow);
        }
        flows.sort_by_key(|flow| flow.version.as_u128());
        Ok(flows)
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
        let (start, end) = Self::flow_version_prefix_bounds(flow_id);
        let version_keys = {
            let table = txn.open_table(FLOW_VERSIONS_TABLE).map_err(s)?;
            let mut keys = Vec::new();
            for entry in table.range(start.as_str()..end.as_str()).map_err(s)? {
                let (k, _) = entry.map_err(s)?;
                keys.push(k.value().to_string());
            }
            keys
        };
        {
            let mut table = txn.open_table(FLOW_VERSIONS_TABLE).map_err(s)?;
            for key in version_keys {
                table.remove(key.as_str()).map_err(s)?;
            }
        }
        txn.commit().map_err(s)?;
        Ok(())
    }

    async fn save_wasm_module(&self, hash: &str, wasm_bytes: &[u8]) -> Result<()> {
        let txn = self.db.begin_write().map_err(s)?;
        {
            let mut table = txn.open_table(WASM_MODULES_TABLE).map_err(s)?;
            table.insert(hash, wasm_bytes).map_err(s)?;
        }
        txn.commit().map_err(s)?;
        Ok(())
    }

    async fn get_wasm_module(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        let txn = self.db.begin_read().map_err(s)?;
        let table = txn.open_table(WASM_MODULES_TABLE).map_err(s)?;
        match table.get(hash).map_err(s)? {
            Some(data) => Ok(Some(data.value().to_vec())),
            None => Ok(None),
        }
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

    async fn delete_job(&self, job_id: Uuid) -> Result<()> {
        let txn = self.db.begin_write().map_err(s)?;
        {
            let mut table = txn.open_table(JOBS_TABLE).map_err(s)?;
            table.remove(job_id.as_bytes().as_slice()).map_err(s)?;
        }

        let event_keys = {
            let table = txn.open_table(EVENTS_TABLE).map_err(s)?;
            let mut keys = Vec::new();
            let prefix = job_id.as_bytes();
            for entry in table.iter().map_err(s)? {
                let (k, _) = entry.map_err(s)?;
                let key_bytes: &[u8] = k.value();
                if key_bytes.len() >= 16 && key_bytes[..16] == *prefix.as_slice() {
                    keys.push(key_bytes.to_vec());
                }
            }
            keys
        };
        {
            let mut table = txn.open_table(EVENTS_TABLE).map_err(s)?;
            for key in event_keys {
                table.remove(key.as_slice()).map_err(s)?;
            }
        }
        txn.commit().map_err(s)?;
        Ok(())
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
        let (start, end) = Self::event_key_range(job_id);
        let mut events = Vec::new();
        for entry in table.range(start.as_slice()..=end.as_slice()).map_err(s)? {
            let (_, v) = entry.map_err(s)?;
            let event: EventRecord = serde_json::from_slice(v.value()).map_err(s)?;
            events.push(event);
        }
        // 对外统一按事件发生时间返回，避免上层依赖底层 B-tree 迭代顺序。
        events.sort_by_key(|e: &EventRecord| e.timestamp_ms);
        Ok(events)
    }
}

impl CapabilityStore for RedbStorage {
    fn get_value(&self, namespace: &str, key: &str) -> Result<Option<Vec<u8>>> {
        let txn = self.db.begin_read().map_err(s)?;
        let table = txn.open_table(KV_TABLE).map_err(s)?;
        match table
            .get(Self::kv_key(namespace, key).as_str())
            .map_err(s)?
        {
            Some(value) => Ok(Some(value.value().to_vec())),
            None => Ok(None),
        }
    }

    fn put_value(&self, namespace: &str, key: &str, value: &[u8]) -> Result<()> {
        let txn = self.db.begin_write().map_err(s)?;
        {
            let mut table = txn.open_table(KV_TABLE).map_err(s)?;
            table
                .insert(Self::kv_key(namespace, key).as_str(), value)
                .map_err(s)?;
        }
        txn.commit().map_err(s)?;
        Ok(())
    }

    fn delete_value(&self, namespace: &str, key: &str) -> Result<bool> {
        let txn = self.db.begin_write().map_err(s)?;
        let removed = {
            let mut table = txn.open_table(KV_TABLE).map_err(s)?;
            table
                .remove(Self::kv_key(namespace, key).as_str())
                .map_err(s)?
                .is_some()
        };
        txn.commit().map_err(s)?;
        Ok(removed)
    }

    fn list_keys(
        &self,
        namespace: &str,
        prefix: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<String>> {
        let txn = self.db.begin_read().map_err(s)?;
        let table = txn.open_table(KV_TABLE).map_err(s)?;
        let namespace_prefix = format!("{namespace}\u{0}");
        let key_prefix = prefix.unwrap_or_default();
        let mut keys = Vec::new();
        for entry in table.iter().map_err(s)? {
            let (raw_key, _) = entry.map_err(s)?;
            let raw_key = raw_key.value();
            if !raw_key.starts_with(&namespace_prefix) {
                continue;
            }
            let key = &raw_key[namespace_prefix.len()..];
            if !key.starts_with(key_prefix) {
                continue;
            }
            keys.push(key.to_string());
            if limit.is_some_and(|limit| keys.len() >= limit as usize) {
                break;
            }
        }
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use shiroha_core::event::{EventKind, EventRecord};
    use shiroha_core::flow::{
        ActionDef, DispatchMode, FlowManifest, FlowRegistration, FlowWorld, StateDef, StateKind,
        TransitionDef,
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
                host_world: FlowWorld::Sandbox,
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
                    capabilities: Vec::new(),
                }],
            },
            wasm_hash: "hash-demo".into(),
        }
    }

    fn sample_flow_registration_with_id(flow_id: &str) -> FlowRegistration {
        FlowRegistration {
            flow_id: flow_id.to_string(),
            ..sample_flow_registration()
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
            pending_events: Vec::new(),
            scheduled_timeouts: vec![shiroha_core::job::ScheduledTimeout {
                event: "expire".into(),
                remaining_ms: 42,
            }],
            timeout_anchor_ms: Some(1_234),
        }
    }

    #[tokio::test]
    async fn persists_flow_job_and_event_across_reopen() {
        let path = temp_db_path("persist");
        let flow = sample_flow_registration();
        let job = sample_job(&flow);
        let wasm_bytes = b"(component)".to_vec();
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
            storage
                .save_wasm_module(&flow.wasm_hash, &wasm_bytes)
                .await
                .expect("save wasm module");
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
        let versioned_flow = reopened
            .get_flow_version(&flow.flow_id, flow.version)
            .await
            .expect("get flow version")
            .expect("flow version exists");
        let stored_job = reopened
            .get_job(job.id)
            .await
            .expect("get job")
            .expect("job exists");
        let events = reopened.get_events(job.id).await.expect("events");
        let jobs = reopened.list_jobs(&flow.flow_id).await.expect("jobs");
        let flows = reopened.list_flows().await.expect("flows");
        let flow_versions = reopened.list_flow_versions().await.expect("flow versions");
        let stored_wasm = reopened
            .get_wasm_module(&flow.wasm_hash)
            .await
            .expect("get wasm module")
            .expect("wasm exists");

        assert_eq!(stored_flow.flow_id, flow.flow_id);
        assert_eq!(versioned_flow.version, flow.version);
        assert_eq!(stored_flow.wasm_hash, "hash-demo");
        assert_eq!(stored_job.current_state, "idle");
        assert_eq!(stored_job.context, Some(vec![1, 2, 3]));
        assert_eq!(stored_job.scheduled_timeouts, job.scheduled_timeouts);
        assert_eq!(stored_job.timeout_anchor_ms, job.timeout_anchor_ms);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].kind, EventKind::Created { .. }));
        assert_eq!(jobs.len(), 1);
        assert_eq!(flows.len(), 1);
        assert_eq!(flow_versions.len(), 1);
        assert_eq!(stored_wasm, wasm_bytes);

        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn list_flows_returns_latest_while_versions_keep_history() {
        let path = temp_db_path("flow-versions");
        let first = sample_flow_registration();
        let second = FlowRegistration {
            version: Uuid::now_v7(),
            wasm_hash: "hash-demo-v2".into(),
            ..first.clone()
        };
        let storage = RedbStorage::new(&path).expect("open db");

        storage.save_flow(&first).await.expect("save first flow");
        storage.save_flow(&second).await.expect("save second flow");

        let latest = storage
            .get_flow(&first.flow_id)
            .await
            .expect("get latest")
            .expect("latest exists");
        let first_version = storage
            .get_flow_version(&first.flow_id, first.version)
            .await
            .expect("get first version")
            .expect("first version exists");
        let second_version = storage
            .get_flow_version(&second.flow_id, second.version)
            .await
            .expect("get second version")
            .expect("second version exists");
        let latest_list = storage.list_flows().await.expect("list latest");
        let all_versions = storage.list_flow_versions().await.expect("list versions");

        assert_eq!(latest.version, second.version);
        assert_eq!(first_version.version, first.version);
        assert_eq!(second_version.version, second.version);
        assert_eq!(latest_list.len(), 1);
        assert_eq!(all_versions.len(), 2);

        drop(storage);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn list_flow_versions_for_returns_only_matching_flow() {
        let path = temp_db_path("flow-versions-for");
        let alpha_v1 = FlowRegistration {
            version: Uuid::from_u128(2),
            ..sample_flow_registration_with_id("alpha")
        };
        let alpha_v2 = FlowRegistration {
            version: Uuid::from_u128(1),
            wasm_hash: "hash-alpha-v2".into(),
            ..alpha_v1.clone()
        };
        let beta_v1 = FlowRegistration {
            version: Uuid::from_u128(9),
            ..sample_flow_registration_with_id("beta")
        };
        let alpha_null_beta_v1 = FlowRegistration {
            version: Uuid::from_u128(8),
            ..sample_flow_registration_with_id("alpha\0beta")
        };
        let storage = RedbStorage::new(&path).expect("open db");

        storage.save_flow(&alpha_v1).await.expect("save alpha v1");
        storage.save_flow(&alpha_v2).await.expect("save alpha v2");
        storage.save_flow(&beta_v1).await.expect("save beta v1");
        storage
            .save_flow(&alpha_null_beta_v1)
            .await
            .expect("save alpha\\0beta v1");

        let versions = storage
            .list_flow_versions_for("alpha")
            .await
            .expect("list alpha versions");

        assert_eq!(versions.len(), 2);
        assert!(versions.iter().all(|flow| flow.flow_id == "alpha"));
        assert_eq!(
            versions.iter().map(|flow| flow.version).collect::<Vec<_>>(),
            vec![alpha_v2.version, alpha_v1.version]
        );

        drop(storage);
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

    #[tokio::test]
    async fn get_events_ignores_records_for_other_jobs_with_neighboring_keys() {
        let path = temp_db_path("event-neighboring-keys");
        let flow = sample_flow_registration();
        let target_job = Job {
            id: Uuid::from_u128(42),
            ..sample_job(&flow)
        };
        let neighbor_job = Job {
            id: Uuid::from_u128(43),
            ..target_job.clone()
        };
        let storage = RedbStorage::new(&path).expect("open db");

        storage
            .save_job(&target_job)
            .await
            .expect("save target job");
        storage
            .save_job(&neighbor_job)
            .await
            .expect("save neighbor job");

        storage
            .append_event(&EventRecord {
                id: Uuid::now_v7(),
                job_id: target_job.id,
                timestamp_ms: 200,
                kind: EventKind::Completed {
                    final_state: "done".into(),
                },
            })
            .await
            .expect("append target event");
        storage
            .append_event(&EventRecord {
                id: Uuid::now_v7(),
                job_id: neighbor_job.id,
                timestamp_ms: 100,
                kind: EventKind::Transition {
                    event: "finish".into(),
                    from: "idle".into(),
                    to: "done".into(),
                    action: Some("ship".into()),
                },
            })
            .await
            .expect("append neighbor event");

        let events = storage
            .get_events(target_job.id)
            .await
            .expect("get target events");

        assert_eq!(events.len(), 1);
        assert!(events.iter().all(|event| event.job_id == target_job.id));
        assert!(matches!(events[0].kind, EventKind::Completed { .. }));

        drop(storage);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn delete_flow_removes_latest_and_version_history() {
        let path = temp_db_path("delete-flow");
        let first = sample_flow_registration();
        let second = FlowRegistration {
            version: Uuid::now_v7(),
            wasm_hash: "hash-demo-v2".into(),
            ..first.clone()
        };
        let storage = RedbStorage::new(&path).expect("open db");

        storage.save_flow(&first).await.expect("save first flow");
        storage.save_flow(&second).await.expect("save second flow");
        storage
            .delete_flow(&first.flow_id)
            .await
            .expect("delete flow");

        assert!(
            storage
                .get_flow(&first.flow_id)
                .await
                .expect("get latest")
                .is_none()
        );
        assert!(storage.list_flows().await.expect("list flows").is_empty());
        assert!(
            storage
                .list_flow_versions()
                .await
                .expect("list versions")
                .is_empty()
        );

        drop(storage);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn delete_job_removes_snapshot_and_events() {
        let path = temp_db_path("delete-job");
        let flow = sample_flow_registration();
        let job = sample_job(&flow);
        let storage = RedbStorage::new(&path).expect("open db");

        storage.save_job(&job).await.expect("save job");
        storage
            .append_event(&EventRecord {
                id: Uuid::now_v7(),
                job_id: job.id,
                timestamp_ms: 100,
                kind: EventKind::Created {
                    flow_id: flow.flow_id.clone(),
                    flow_version: flow.version,
                    initial_state: flow.manifest.initial_state.clone(),
                },
            })
            .await
            .expect("append event");

        storage.delete_job(job.id).await.expect("delete job");

        assert!(storage.get_job(job.id).await.expect("get job").is_none());
        assert!(
            storage
                .get_events(job.id)
                .await
                .expect("get events")
                .is_empty()
        );

        drop(storage);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn capability_store_round_trip_works_in_redb() {
        let path = temp_db_path("kv-store");
        let storage = RedbStorage::new(&path).expect("open db");

        storage
            .put_value("fixture", "alpha", b"one")
            .expect("put alpha");
        storage
            .put_value("fixture", "beta", b"two")
            .expect("put beta");

        assert_eq!(
            storage.get_value("fixture", "alpha").expect("get alpha"),
            Some(b"one".to_vec())
        );
        assert_eq!(
            storage
                .list_keys("fixture", Some("a"), None)
                .expect("list keys"),
            vec!["alpha".to_string()]
        );
        assert!(
            storage
                .delete_value("fixture", "alpha")
                .expect("delete alpha")
        );
        assert_eq!(
            storage.get_value("fixture", "alpha").expect("get alpha"),
            None
        );

        let _ = std::fs::remove_file(path);
    }
}
