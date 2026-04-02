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

fn s(e: impl std::fmt::Display) -> ShirohaError {
    ShirohaError::Storage(e.to_string())
}

pub struct RedbStorage {
    db: Arc<Database>,
}

impl RedbStorage {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
        let db = Database::create(path).map_err(s)?;
        let txn = db.begin_write().map_err(s)?;
        let _ = txn.open_table(FLOWS_TABLE).map_err(s)?;
        let _ = txn.open_table(JOBS_TABLE).map_err(s)?;
        let _ = txn.open_table(EVENTS_TABLE).map_err(s)?;
        txn.commit().map_err(s)?;
        Ok(Self { db: Arc::new(db) })
    }

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
            table.insert(flow.flow_id.as_str(), data.as_slice()).map_err(s)?;
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
            table.insert(job.id.as_bytes().as_slice(), data.as_slice()).map_err(s)?;
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
        for entry in table.iter().map_err(s)? {
            let (k, v) = entry.map_err(s)?;
            let key_bytes: &[u8] = k.value();
            if key_bytes.len() >= 16 && key_bytes[..16] == *prefix.as_slice() {
                let event: EventRecord = serde_json::from_slice(v.value()).map_err(s)?;
                events.push(event);
            }
        }
        events.sort_by_key(|e: &EventRecord| e.timestamp_ms);
        Ok(events)
    }
}
