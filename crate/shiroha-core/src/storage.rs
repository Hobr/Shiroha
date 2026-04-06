//! 存储层抽象
//!
//! 定义 [`Storage`] trait 作为持久化后端的统一接口。
//! 内置 [`MemoryStorage`] 用于开发和测试。
//! 生产环境使用 `shiroha-store-redb` 等具体实现。

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::RwLock as StdRwLock;

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::Result;
use crate::event::EventRecord;
use crate::flow::FlowRegistration;
use crate::job::Job;

type MemoryKvMap = BTreeMap<(String, String), Vec<u8>>;

/// 存储后端 trait
///
/// 所有方法均为异步，实现方必须保证 Send + Sync。
/// Flow、Job、Event 三类数据分别存储。
pub trait Storage: Send + Sync {
    fn save_flow(&self, flow: &FlowRegistration) -> impl Future<Output = Result<()>> + Send;
    fn get_flow(
        &self,
        flow_id: &str,
    ) -> impl Future<Output = Result<Option<FlowRegistration>>> + Send;
    fn get_flow_version(
        &self,
        flow_id: &str,
        version: Uuid,
    ) -> impl Future<Output = Result<Option<FlowRegistration>>> + Send;
    fn list_flow_versions(&self) -> impl Future<Output = Result<Vec<FlowRegistration>>> + Send;
    fn list_flows(&self) -> impl Future<Output = Result<Vec<FlowRegistration>>> + Send;
    fn delete_flow(&self, flow_id: &str) -> impl Future<Output = Result<()>> + Send;
    fn save_wasm_module(
        &self,
        hash: &str,
        wasm_bytes: &[u8],
    ) -> impl Future<Output = Result<()>> + Send;
    fn get_wasm_module(&self, hash: &str) -> impl Future<Output = Result<Option<Vec<u8>>>> + Send;

    fn save_job(&self, job: &Job) -> impl Future<Output = Result<()>> + Send;
    /// 同时写入 Job 快照和事件记录。
    ///
    /// 默认实现按顺序调用 `save_job` 和 `append_event`。
    /// 只有后端覆写该方法时，才能把两次写入合并为单个原子提交。
    fn save_job_with_event(
        &self,
        job: &Job,
        event: &EventRecord,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            self.save_job(job).await?;
            self.append_event(event).await
        }
    }
    fn get_job(&self, job_id: Uuid) -> impl Future<Output = Result<Option<Job>>> + Send;
    fn list_jobs(&self, flow_id: &str) -> impl Future<Output = Result<Vec<Job>>> + Send;
    fn delete_job(&self, job_id: Uuid) -> impl Future<Output = Result<()>> + Send;

    fn append_event(&self, event: &EventRecord) -> impl Future<Output = Result<()>> + Send;
    fn get_events(&self, job_id: Uuid) -> impl Future<Output = Result<Vec<EventRecord>>> + Send;
}

/// 面向 WASM capability host 的同步 KV 存储抽象。
///
/// 这里选择 object-safe 同步 API，便于在 component host import 中通过 trait object 直接调用。
pub trait CapabilityStore: Send + Sync {
    fn get_value(&self, namespace: &str, key: &str) -> Result<Option<Vec<u8>>>;
    fn put_value(&self, namespace: &str, key: &str, value: &[u8]) -> Result<()>;
    fn delete_value(&self, namespace: &str, key: &str) -> Result<bool>;
    fn list_keys(
        &self,
        namespace: &str,
        prefix: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<String>>;
}

/// 基于内存的存储实现（开发/测试用）
///
/// 数据仅存活于进程生命周期内，不做持久化。
#[derive(Debug, Default, Clone)]
pub struct MemoryStorage {
    flows: Arc<RwLock<HashMap<String, FlowRegistration>>>,
    flow_versions: Arc<RwLock<HashMap<(String, Uuid), FlowRegistration>>>,
    wasm_modules: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    jobs: Arc<RwLock<HashMap<Uuid, Job>>>,
    /// 事件按追加顺序保存在内存中，测试可以直接断言生命周期顺序。
    events: Arc<RwLock<Vec<EventRecord>>>,
    kv: Arc<StdRwLock<MemoryKvMap>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Storage for MemoryStorage {
    async fn save_flow(&self, flow: &FlowRegistration) -> Result<()> {
        self.flow_versions
            .write()
            .await
            .insert((flow.flow_id.clone(), flow.version), flow.clone());
        self.flows
            .write()
            .await
            .insert(flow.flow_id.clone(), flow.clone());
        Ok(())
    }

    async fn get_flow(&self, flow_id: &str) -> Result<Option<FlowRegistration>> {
        Ok(self.flows.read().await.get(flow_id).cloned())
    }

    async fn get_flow_version(
        &self,
        flow_id: &str,
        version: Uuid,
    ) -> Result<Option<FlowRegistration>> {
        Ok(self
            .flow_versions
            .read()
            .await
            .get(&(flow_id.to_string(), version))
            .cloned())
    }

    async fn list_flow_versions(&self) -> Result<Vec<FlowRegistration>> {
        Ok(self.flow_versions.read().await.values().cloned().collect())
    }

    async fn list_flows(&self) -> Result<Vec<FlowRegistration>> {
        Ok(self.flows.read().await.values().cloned().collect())
    }

    async fn delete_flow(&self, flow_id: &str) -> Result<()> {
        self.flows.write().await.remove(flow_id);
        self.flow_versions
            .write()
            .await
            .retain(|(candidate, _), _| candidate != flow_id);
        Ok(())
    }

    async fn save_wasm_module(&self, hash: &str, wasm_bytes: &[u8]) -> Result<()> {
        self.wasm_modules
            .write()
            .await
            .insert(hash.to_string(), wasm_bytes.to_vec());
        Ok(())
    }

    async fn get_wasm_module(&self, hash: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.wasm_modules.read().await.get(hash).cloned())
    }

    async fn save_job(&self, job: &Job) -> Result<()> {
        self.jobs.write().await.insert(job.id, job.clone());
        Ok(())
    }

    async fn get_job(&self, job_id: Uuid) -> Result<Option<Job>> {
        Ok(self.jobs.read().await.get(&job_id).cloned())
    }

    async fn list_jobs(&self, flow_id: &str) -> Result<Vec<Job>> {
        Ok(self
            .jobs
            .read()
            .await
            .values()
            .filter(|j| j.flow_id == flow_id)
            .cloned()
            .collect())
    }

    async fn delete_job(&self, job_id: Uuid) -> Result<()> {
        self.jobs.write().await.remove(&job_id);
        self.events
            .write()
            .await
            .retain(|event| event.job_id != job_id);
        Ok(())
    }

    async fn append_event(&self, event: &EventRecord) -> Result<()> {
        self.events.write().await.push(event.clone());
        Ok(())
    }

    async fn get_events(&self, job_id: Uuid) -> Result<Vec<EventRecord>> {
        Ok(self
            .events
            .read()
            .await
            .iter()
            .filter(|e| e.job_id == job_id)
            .cloned()
            .collect())
    }
}

impl CapabilityStore for MemoryStorage {
    fn get_value(&self, namespace: &str, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self
            .kv
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&(namespace.to_string(), key.to_string()))
            .cloned())
    }

    fn put_value(&self, namespace: &str, key: &str, value: &[u8]) -> Result<()> {
        self.kv
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert((namespace.to_string(), key.to_string()), value.to_vec());
        Ok(())
    }

    fn delete_value(&self, namespace: &str, key: &str) -> Result<bool> {
        Ok(self
            .kv
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&(namespace.to_string(), key.to_string()))
            .is_some())
    }

    fn list_keys(
        &self,
        namespace: &str,
        prefix: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<String>> {
        let mut keys = self
            .kv
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .keys()
            .filter(|(entry_namespace, key)| {
                entry_namespace == namespace && prefix.is_none_or(|prefix| key.starts_with(prefix))
            })
            .map(|(_, key)| key.clone())
            .collect::<Vec<_>>();
        if let Some(limit) = limit {
            keys.truncate(limit as usize);
        }
        Ok(keys)
    }
}

#[cfg(test)]
mod tests {
    use super::{CapabilityStore, MemoryStorage};

    #[test]
    fn capability_store_round_trip_works_in_memory() {
        let storage = MemoryStorage::new();

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
    }
}
