//! gRPC FlowService 实现
//!
//! 处理 Flow 的部署、查询操作。
//! 部署时：加载 WASM → 提取 manifest → 静态验证 → 持久化 → 缓存引擎。

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
    /// 部署 Flow：接收 WASM 字节码，编译、验证、注册
    async fn deploy_flow(
        &self,
        request: Request<DeployFlowRequest>,
    ) -> Result<Response<DeployFlowResponse>, Status> {
        let req = request.into_inner();
        let flow_id = req.flow_id;
        let wasm_bytes = req.wasm_bytes;

        // 编译 WASM 模块
        let module = self
            .state
            .wasm_runtime
            .load_module(&wasm_bytes)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let wasm_module = Arc::new(WasmModule::new(module, &wasm_bytes));

        // 从 WASM 提取 manifest
        let mut host = shiroha_wasm::host::WasmHost::new(
            self.state.wasm_runtime.engine(),
            wasm_module.module(),
        )
        .map_err(|e| Status::internal(e.to_string()))?;

        let manifest = host
            .get_manifest()
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        // 静态验证
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

        // 持久化 + 缓存
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TestHarness, approval_manifest, wasm_for_manifest};

    #[tokio::test]
    async fn deploy_list_and_get_flow_round_trip() {
        let harness = TestHarness::new("flow-service").await;
        let service = FlowServiceImpl::new(harness.state.clone());
        let manifest = approval_manifest("demo-flow", Some("allow"));

        let deploy = service
            .deploy_flow(Request::new(DeployFlowRequest {
                flow_id: "demo-flow".into(),
                wasm_bytes: wasm_for_manifest(&manifest),
            }))
            .await
            .expect("deploy flow")
            .into_inner();

        assert_eq!(deploy.flow_id, "demo-flow");
        assert!(deploy.version.parse::<Uuid>().is_ok());

        let listed = service
            .list_flows(Request::new(ListFlowsRequest {}))
            .await
            .expect("list flows")
            .into_inner();
        assert_eq!(listed.flows.len(), 1);
        assert_eq!(listed.flows[0].flow_id, "demo-flow");
        assert_eq!(listed.flows[0].initial_state, "idle");
        assert_eq!(listed.flows[0].state_count, 2);

        let fetched = service
            .get_flow(Request::new(GetFlowRequest {
                flow_id: "demo-flow".into(),
            }))
            .await
            .expect("get flow")
            .into_inner();

        assert_eq!(fetched.flow_id, "demo-flow");
        let fetched_manifest: shiroha_core::flow::FlowManifest =
            serde_json::from_str(&fetched.manifest_json).expect("manifest json");
        assert_eq!(fetched_manifest.initial_state, "idle");
        assert_eq!(fetched_manifest.transitions.len(), 1);
        assert!(
            harness
                .state
                .module_cache
                .get(
                    &harness
                        .state
                        .storage
                        .get_flow("demo-flow")
                        .await
                        .expect("stored flow")
                        .expect("flow exists")
                        .wasm_hash
                )
                .is_some()
        );
    }

    #[tokio::test]
    async fn deploy_flow_rejects_invalid_wasm_contract() {
        let harness = TestHarness::new("flow-invalid").await;
        let service = FlowServiceImpl::new(harness.state.clone());

        let error = service
            .deploy_flow(Request::new(DeployFlowRequest {
                flow_id: "broken".into(),
                wasm_bytes: b"(module)".to_vec(),
            }))
            .await
            .expect_err("missing exports should fail");

        assert_eq!(error.code(), tonic::Code::InvalidArgument);
    }
}
