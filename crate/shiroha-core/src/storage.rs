//! 存储层抽象
//!
//! 定义 [`Storage`] trait 作为持久化后端的统一接口。
//! 内置 [`MemoryStorage`] 用于开发和测试。
//! 生产环境使用 `shiroha-store-redb` 等具体实现。

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::Result;
use crate::event::EventRecord;
use crate::flow::FlowRegistration;
use crate::job::Job;

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
    fn list_flows(&self) -> impl Future<Output = Result<Vec<FlowRegistration>>> + Send;
    fn delete_flow(&self, flow_id: &str) -> impl Future<Output = Result<()>> + Send;

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

    fn append_event(&self, event: &EventRecord) -> impl Future<Output = Result<()>> + Send;
    fn get_events(&self, job_id: Uuid) -> impl Future<Output = Result<Vec<EventRecord>>> + Send;
}

/// 基于内存的存储实现（开发/测试用）
///
/// 数据仅存活于进程生命周期内，不做持久化。
#[derive(Debug, Default, Clone)]
pub struct MemoryStorage {
    flows: Arc<RwLock<HashMap<String, FlowRegistration>>>,
    jobs: Arc<RwLock<HashMap<Uuid, Job>>>,
    /// 事件按追加顺序保存在内存中，测试可以直接断言生命周期顺序。
    events: Arc<RwLock<Vec<EventRecord>>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Storage for MemoryStorage {
    async fn save_flow(&self, flow: &FlowRegistration) -> Result<()> {
        self.flows
            .write()
            .await
            .insert(flow.flow_id.clone(), flow.clone());
        Ok(())
    }

    async fn get_flow(&self, flow_id: &str) -> Result<Option<FlowRegistration>> {
        Ok(self.flows.read().await.get(flow_id).cloned())
    }

    async fn list_flows(&self) -> Result<Vec<FlowRegistration>> {
        Ok(self.flows.read().await.values().cloned().collect())
    }

    async fn delete_flow(&self, flow_id: &str) -> Result<()> {
        self.flows.write().await.remove(flow_id);
        Ok(())
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
