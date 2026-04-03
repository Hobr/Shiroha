//! 服务器核心：初始化共享状态，启动 gRPC 服务

use std::collections::{HashMap, VecDeque};
#[cfg(test)]
use std::future::Future;
use std::sync::Arc;

use shiroha_core::flow::FlowRegistration;
use shiroha_core::storage::Storage;
use shiroha_engine::engine::StateMachineEngine;
use shiroha_engine::job::JobManager;
use shiroha_engine::timer::TimerWheel;
use shiroha_store_redb::store::RedbStorage;
use shiroha_wasm::module_cache::ModuleCache;
use shiroha_wasm::module_cache::WasmModule;
use shiroha_wasm::runtime::WasmRuntime;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
#[cfg(test)]
use tokio_stream::wrappers::UnixListenerStream;
use uuid::Uuid;

use crate::flow_service::FlowServiceImpl;
use crate::job_service::JobServiceImpl;

/// 全局共享状态，所有 gRPC handler 共享
pub struct ShirohaState {
    pub storage: Arc<RedbStorage>,
    pub wasm_runtime: Arc<WasmRuntime>,
    pub module_cache: Arc<ModuleCache>,
    /// 内存中的最新 Flow 注册表（flow_id → FlowRegistration）。
    /// 部署后会落盘到 storage，这里保留一份热路径缓存，避免每次请求都查数据库。
    pub flows: Arc<Mutex<HashMap<String, FlowRegistration>>>,
    /// 所有已部署版本的 Flow 注册表（(flow_id, version) → FlowRegistration）。
    pub flow_versions: Arc<Mutex<HashMap<(String, Uuid), FlowRegistration>>>,
    /// 每个 Flow 最新版本对应的状态机引擎（flow_id → Engine）。
    /// Engine 是只读拓扑索引，常驻内存后事件处理只需要查 HashMap。
    pub engines: Arc<Mutex<HashMap<String, StateMachineEngine>>>,
    /// 所有已部署版本的状态机引擎（(flow_id, version) → Engine）。
    pub versioned_engines: Arc<Mutex<HashMap<(String, Uuid), StateMachineEngine>>>,
    /// Job 级串行化锁，保证同一 Job 任一时刻只处理一个事件/生命周期操作
    pub(crate) job_locks: Arc<Mutex<HashMap<uuid::Uuid, Arc<Mutex<()>>>>>,
    /// 暂停期间缓存的待处理事件，恢复后按 FIFO 继续处理
    pub(crate) pending_events:
        Arc<Mutex<HashMap<uuid::Uuid, VecDeque<crate::job_service::QueuedJobEvent>>>>,
    pub job_manager: Arc<JobManager<RedbStorage>>,
    pub timer_wheel: Arc<TimerWheel>,
}

pub struct ShirohaServer {
    state: Arc<ShirohaState>,
    /// 定时器事件接收端
    timer_rx: tokio::sync::mpsc::Receiver<shiroha_engine::timer::TimerEvent>,
}

pub(crate) fn spawn_timer_forwarder(
    state: Arc<ShirohaState>,
    mut timer_rx: tokio::sync::mpsc::Receiver<shiroha_engine::timer::TimerEvent>,
) -> JoinHandle<()> {
    let timer_job_svc = JobServiceImpl::new(state);
    tokio::spawn(async move {
        while let Some(timer_event) = timer_rx.recv().await {
            let event_name = timer_event.event.clone();
            // 定时器事件和外部 trigger-event 共用同一条 enqueue 路径，
            // 这样串行化、暂停队列和审计行为都保持一致。
            if let Err(error) = timer_job_svc
                .enqueue_event(timer_event.job_id, timer_event.event, None)
                .await
            {
                tracing::error!(
                    job_id = %timer_event.job_id,
                    event = event_name,
                    error = %error,
                    "failed to process timer event"
                );
            }
        }
    })
}

impl ShirohaServer {
    /// 初始化所有组件：存储、WASM 运行时、定时器轮
    pub async fn new(data_dir: &str) -> anyhow::Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let db_path = format!("{data_dir}/shiroha.redb");

        let storage = Arc::new(RedbStorage::new(&db_path).map_err(|e| anyhow::anyhow!("{e}"))?);
        let wasm_runtime = Arc::new(WasmRuntime::new().map_err(|e| anyhow::anyhow!("{e}"))?);
        let module_cache = Arc::new(ModuleCache::new());
        let job_manager = Arc::new(JobManager::new(storage.clone()));
        let (timer_wheel, timer_rx) = TimerWheel::new();

        let state = Arc::new(ShirohaState {
            storage,
            wasm_runtime,
            module_cache,
            flows: Arc::new(Mutex::new(HashMap::new())),
            flow_versions: Arc::new(Mutex::new(HashMap::new())),
            engines: Arc::new(Mutex::new(HashMap::new())),
            versioned_engines: Arc::new(Mutex::new(HashMap::new())),
            job_locks: Arc::new(Mutex::new(HashMap::new())),
            pending_events: Arc::new(Mutex::new(HashMap::new())),
            job_manager,
            timer_wheel: Arc::new(timer_wheel),
        });

        let persisted_flows = state
            .storage
            .list_flow_versions()
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        if !persisted_flows.is_empty() {
            let mut flows = state.flows.lock().await;
            let mut flow_versions = state.flow_versions.lock().await;
            let mut engines = state.engines.lock().await;
            let mut versioned_engines = state.versioned_engines.lock().await;
            // 重启后恢复的是 Flow 拓扑和编译入口，不包括运行中的 timer 状态；
            // timer 会在后续 Job 创建或状态迁移时重新注册。
            for registration in persisted_flows {
                let wasm_bytes = state
                    .storage
                    .get_wasm_module(&registration.wasm_hash)
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing wasm bytes for flow `{}` version {}",
                            registration.flow_id,
                            registration.version
                        )
                    })?;
                if state.module_cache.get(&registration.wasm_hash).is_none() {
                    let component = state
                        .wasm_runtime
                        .load_component(&wasm_bytes)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    state
                        .module_cache
                        .insert(Arc::new(WasmModule::new(component, &wasm_bytes)));
                }

                let versioned_key = (registration.flow_id.clone(), registration.version);
                flow_versions.insert(versioned_key.clone(), registration.clone());
                versioned_engines.insert(
                    versioned_key,
                    StateMachineEngine::new(registration.manifest.clone()),
                );

                let should_replace_latest = flows
                    .get(&registration.flow_id)
                    .is_none_or(|existing| registration.version > existing.version);
                if should_replace_latest {
                    engines.insert(
                        registration.flow_id.clone(),
                        StateMachineEngine::new(registration.manifest.clone()),
                    );
                    flows.insert(registration.flow_id.clone(), registration);
                }
            }
            tracing::info!(
                latest_count = flows.len(),
                version_count = flow_versions.len(),
                "loaded persisted flows into memory registry"
            );
        }

        Ok(Self { state, timer_rx })
    }

    /// 启动 gRPC 服务器
    pub async fn start(mut self, listen_addr: &str) -> anyhow::Result<()> {
        let addr = listen_addr.parse()?;

        let flow_svc = FlowServiceImpl::new(self.state.clone());
        let job_svc = JobServiceImpl::new(self.state.clone());

        // Receiver 只能被一个 forwarder 持有一次，因此先把真实的 timer_rx 移走。
        let timer_rx = std::mem::replace(&mut self.timer_rx, tokio::sync::mpsc::channel(1).1);
        let _timer_forwarder = spawn_timer_forwarder(self.state.clone(), timer_rx);

        tracing::info!(%addr, "gRPC server listening");

        tonic::transport::Server::builder()
            .add_service(
                shiroha_proto::shiroha_api::flow_service_server::FlowServiceServer::new(flow_svc),
            )
            .add_service(
                shiroha_proto::shiroha_api::job_service_server::JobServiceServer::new(job_svc),
            )
            .serve(addr)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
impl ShirohaServer {
    pub(crate) async fn start_with_unix_listener<F>(
        mut self,
        listener: tokio::net::UnixListener,
        shutdown: F,
    ) -> anyhow::Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let flow_svc = FlowServiceImpl::new(self.state.clone());
        let job_svc = JobServiceImpl::new(self.state.clone());
        let timer_rx = std::mem::replace(&mut self.timer_rx, tokio::sync::mpsc::channel(1).1);
        let _timer_forwarder = spawn_timer_forwarder(self.state.clone(), timer_rx);

        tonic::transport::Server::builder()
            .add_service(
                shiroha_proto::shiroha_api::flow_service_server::FlowServiceServer::new(flow_svc),
            )
            .add_service(
                shiroha_proto::shiroha_api::job_service_server::JobServiceServer::new(job_svc),
            )
            .serve_with_incoming_shutdown(UnixListenerStream::new(listener), shutdown)
            .await?;

        Ok(())
    }

    pub(crate) fn into_test_parts(
        self,
    ) -> (
        Arc<ShirohaState>,
        tokio::sync::mpsc::Receiver<shiroha_engine::timer::TimerEvent>,
    ) {
        (self.state, self.timer_rx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow_service::FlowServiceImpl;
    use crate::job_service::JobServiceImpl;
    use crate::test_support::{TestHarness, approval_manifest, wasm_for_manifest};
    use shiroha_proto::shiroha_api::flow_service_server::FlowService;
    use shiroha_proto::shiroha_api::job_service_server::JobService;
    use shiroha_proto::shiroha_api::{
        CreateJobRequest, DeployFlowRequest, GetJobRequest, TriggerEventRequest,
    };
    use tonic::Request;

    #[tokio::test]
    async fn new_server_loads_persisted_flows_into_memory_registry() {
        let harness = TestHarness::new("server-reload").await;
        let data_dir = harness.data_dir.clone();
        let flow_service = FlowServiceImpl::new(harness.state.clone());

        flow_service
            .deploy_flow(Request::new(DeployFlowRequest {
                flow_id: "persisted".into(),
                wasm_bytes: wasm_for_manifest(&approval_manifest("persisted", Some("allow"))),
            }))
            .await
            .expect("deploy flow");

        drop(flow_service);
        drop(harness);

        let reloaded = ShirohaServer::new(data_dir.to_str().expect("utf-8 path"))
            .await
            .expect("reload server");

        assert!(reloaded.state.flows.lock().await.contains_key("persisted"));
        assert!(
            reloaded
                .state
                .engines
                .lock()
                .await
                .contains_key("persisted")
        );
        assert!(
            reloaded
                .state
                .flow_versions
                .lock()
                .await
                .keys()
                .any(|(flow_id, _)| flow_id == "persisted")
        );
        let wasm_hash = reloaded
            .state
            .storage
            .get_flow("persisted")
            .await
            .expect("flow")
            .expect("persisted flow")
            .wasm_hash;
        assert!(reloaded.state.module_cache.get(&wasm_hash).is_some());
    }

    #[tokio::test]
    async fn reloaded_server_can_continue_existing_job_execution() {
        let harness = TestHarness::new("server-reload-job").await;
        let data_dir = harness.data_dir.clone();
        let flow_service = FlowServiceImpl::new(harness.state.clone());
        let job_service = JobServiceImpl::new(harness.state.clone());

        flow_service
            .deploy_flow(Request::new(DeployFlowRequest {
                flow_id: "persisted".into(),
                wasm_bytes: wasm_for_manifest(&approval_manifest("persisted", Some("allow"))),
            }))
            .await
            .expect("deploy flow");
        let created = job_service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "persisted".into(),
                context: None,
            }))
            .await
            .expect("create job")
            .into_inner();

        drop(job_service);
        drop(flow_service);
        drop(harness);

        let reloaded = ShirohaServer::new(data_dir.to_str().expect("utf-8 path"))
            .await
            .expect("reload server");
        let job_service = JobServiceImpl::new(reloaded.state.clone());

        job_service
            .trigger_event(Request::new(TriggerEventRequest {
                job_id: created.job_id.clone(),
                event: "approve".into(),
                payload: None,
            }))
            .await
            .expect("trigger event after reload");

        let job = job_service
            .get_job(Request::new(GetJobRequest {
                job_id: created.job_id,
            }))
            .await
            .expect("get job")
            .into_inner();
        assert_eq!(job.state, "completed");
        assert_eq!(job.current_state, "done");
    }
}
