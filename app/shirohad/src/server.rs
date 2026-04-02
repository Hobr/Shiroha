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

pub struct ShirohaState {
    pub storage: Arc<RedbStorage>,
    pub wasm_runtime: Arc<WasmRuntime>,
    pub module_cache: Arc<ModuleCache>,
    pub flows: Arc<Mutex<HashMap<String, FlowRegistration>>>,
    pub engines: Arc<Mutex<HashMap<String, StateMachineEngine>>>,
    pub job_manager: Arc<JobManager<RedbStorage>>,
    pub timer_wheel: Arc<TimerWheel>,
}

pub struct ShirohaServer {
    state: Arc<ShirohaState>,
    _timer_rx: tokio::sync::mpsc::Receiver<shiroha_engine::timer::TimerEvent>,
}

impl ShirohaServer {
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
