use std::sync::Arc;

use shiroha_core::flow::FlowRegistration;
use shiroha_core::storage::Storage;
use shiroha_engine::engine::StateMachineEngine;
use shiroha_engine::validator::FlowValidator;
use shiroha_proto::shiroha_api::flow_service_server::FlowService;
use shiroha_proto::shiroha_api::{
    DeployFlowRequest, DeployFlowResponse, FlowSummary, GetFlowRequest, GetFlowResponse,
    ListFlowsRequest, ListFlowsResponse,
};
use shiroha_wasm::module_cache::WasmModule;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::server::ShirohaState;

pub struct FlowServiceImpl {
    state: Arc<ShirohaState>,
}

impl FlowServiceImpl {
    pub fn new(state: Arc<ShirohaState>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl FlowService for FlowServiceImpl {
    async fn deploy_flow(
        &self,
        request: Request<DeployFlowRequest>,
    ) -> Result<Response<DeployFlowResponse>, Status> {
        let req = request.into_inner();
        let flow_id = req.flow_id;
        let wasm_bytes = req.wasm_bytes;

        // Load WASM module
        let module = self
            .state
            .wasm_runtime
            .load_module(&wasm_bytes)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let wasm_module = Arc::new(WasmModule::new(module, &wasm_bytes));

        // Try to get manifest from WASM, fall back to error for now
        let mut host = shiroha_wasm::host::WasmHost::new(
            self.state.wasm_runtime.engine(),
            wasm_module.module(),
        )
        .map_err(|e| Status::internal(e.to_string()))?;

        let manifest = match host.get_manifest() {
            Ok(m) => m,
            Err(_) => {
                return Err(Status::unimplemented(
                    "WASM manifest extraction not yet implemented. Deploy with JSON manifest endpoint in future.",
                ));
            }
        };

        // Validate
        let warnings = FlowValidator::validate(&manifest);
        if !warnings.is_empty() {
            let msgs: Vec<String> = warnings.iter().map(|w| w.to_string()).collect();
            tracing::warn!(flow_id, warnings = ?msgs, "flow validation warnings");
        }

        let version = Uuid::now_v7();
        let registration = FlowRegistration {
            flow_id: flow_id.clone(),
            version,
            manifest: manifest.clone(),
            wasm_hash: wasm_module.hash().to_string(),
        };

        // Store
        self.state
            .storage
            .save_flow(&registration)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        self.state.module_cache.insert(wasm_module);
        self.state
            .flows
            .lock()
            .await
            .insert(flow_id.clone(), registration);
        self.state
            .engines
            .lock()
            .await
            .insert(flow_id.clone(), StateMachineEngine::new(manifest));

        tracing::info!(flow_id, %version, "flow deployed");

        Ok(Response::new(DeployFlowResponse {
            flow_id,
            version: version.to_string(),
        }))
    }

    async fn list_flows(
        &self,
        _request: Request<ListFlowsRequest>,
    ) -> Result<Response<ListFlowsResponse>, Status> {
        let flows = self
            .state
            .storage
            .list_flows()
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        let summaries = flows
            .iter()
            .map(|f| FlowSummary {
                flow_id: f.flow_id.clone(),
                version: f.version.to_string(),
                initial_state: f.manifest.initial_state.clone(),
                state_count: f.manifest.states.len() as u32,
            })
            .collect();

        Ok(Response::new(ListFlowsResponse { flows: summaries }))
    }

    async fn get_flow(
        &self,
        request: Request<GetFlowRequest>,
    ) -> Result<Response<GetFlowResponse>, Status> {
        let flow_id = request.into_inner().flow_id;
        let flow = self
            .state
            .storage
            .get_flow(&flow_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found(format!("flow `{flow_id}` not found")))?;

        let manifest_json =
            serde_json::to_string(&flow.manifest).map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(GetFlowResponse {
            flow_id: flow.flow_id,
            version: flow.version.to_string(),
            manifest_json,
        }))
    }
}
