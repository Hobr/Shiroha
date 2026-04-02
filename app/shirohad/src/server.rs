//! 服务器核心：初始化共享状态，启动 gRPC 服务

use std::collections::HashMap;
use std::sync::Arc;

use shiroha_core::flow::FlowRegistration;
use shiroha_engine::engine::StateMachineEngine;
use shiroha_engine::job::JobManager;
use shiroha_engine::timer::TimerWheel;
use shiroha_store_redb::store::RedbStorage;
use shiroha_wasm::module_cache::ModuleCache;
use shiroha_wasm::runtime::WasmRuntime;
use tokio::sync::Mutex;

use crate::flow_service::FlowServiceImpl;
use crate::job_service::JobServiceImpl;

/// 全局共享状态，所有 gRPC handler 共享
pub struct ShirohaState {
    pub storage: Arc<RedbStorage>,
    pub wasm_runtime: Arc<WasmRuntime>,
    pub module_cache: Arc<ModuleCache>,
    /// 内存中的 Flow 注册表（flow_id → FlowRegistration）
    pub flows: Arc<Mutex<HashMap<String, FlowRegistration>>>,
    /// 每个 Flow 对应的状态机引擎（flow_id → Engine）
    pub engines: Arc<Mutex<HashMap<String, StateMachineEngine>>>,
    pub job_manager: Arc<JobManager<RedbStorage>>,
    pub timer_wheel: Arc<TimerWheel>,
}

pub struct ShirohaServer {
    state: Arc<ShirohaState>,
    /// 定时器事件接收端（TODO: Phase 1 后续接入 Job event inbox）
    _timer_rx: tokio::sync::mpsc::Receiver<shiroha_engine::timer::TimerEvent>,
}

impl ShirohaServer {
    /// 初始化所有组件：存储、WASM 运行时、定时器轮
    pub fn new(data_dir: &str) -> anyhow::Result<Self> {
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
            engines: Arc::new(Mutex::new(HashMap::new())),
            job_manager,
            timer_wheel: Arc::new(timer_wheel),
        });

        Ok(Self {
            state,
            _timer_rx: timer_rx,
        })
    }

    /// 启动 gRPC 服务器
    pub async fn start(self, listen_addr: &str) -> anyhow::Result<()> {
        let addr = listen_addr.parse()?;

        let flow_svc = FlowServiceImpl::new(self.state.clone());
        let job_svc = JobServiceImpl::new(self.state.clone());

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
