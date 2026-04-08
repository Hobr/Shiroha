//! gRPC FlowService 实现
//!
//! 处理 Flow 的部署、查询操作。
//! 部署时：加载 WASM → 提取 manifest → 静态验证 → 持久化 → 缓存引擎。

use std::sync::Arc;

use shiroha_core::flow::{DispatchMode, FlowManifest, FlowRegistration, FlowWorld, StateKind};
use shiroha_core::storage::Storage;
use shiroha_engine::validator::{FlowValidator, ValidationWarning};
use shiroha_proto::shiroha_api::flow_service_server::FlowService;
use shiroha_proto::shiroha_api::{
    DeleteFlowRequest, DeleteFlowResponse, DeployFlowRequest, DeployFlowResponse, FlowSummary,
    GetFlowRequest, GetFlowResponse, ListFlowVersionsRequest, ListFlowVersionsResponse,
    ListFlowsRequest, ListFlowsResponse,
};
use shiroha_wasm::module_cache::WasmModule;
use tonic::{Request, Response, Status};
use uuid::Uuid;

use crate::server::ShirohaState;
use crate::service_support::parse_uuid;

pub struct FlowServiceImpl {
    state: Arc<ShirohaState>,
}

impl FlowServiceImpl {
    const NETWORK_IMPORT: &str = "shiroha:flow/net@0.1.1";
    const STORAGE_IMPORT: &str = "shiroha:flow/store@0.1.1";

    pub fn new(state: Arc<ShirohaState>) -> Self {
        Self { state }
    }

    fn flow_summary(flow: &FlowRegistration) -> FlowSummary {
        FlowSummary {
            flow_id: flow.flow_id.clone(),
            version: flow.version.to_string(),
            initial_state: flow.manifest.initial_state.clone(),
            state_count: flow.manifest.states.len() as u32,
        }
    }

    fn is_fatal_validation_warning(warning: &ValidationWarning) -> bool {
        matches!(
            warning,
            ValidationWarning::InvalidInitialState(_)
                | ValidationWarning::MissingState { .. }
                | ValidationWarning::MissingAction(_)
                | ValidationWarning::MissingGuard(_)
                | ValidationWarning::ActionCapabilityOutsideWorld { .. }
                | ValidationWarning::GuardUsesCapability { .. }
        )
    }

    fn declared_imports_for_world(world: FlowWorld) -> &'static [&'static str] {
        match world {
            FlowWorld::Sandbox => &[],
            FlowWorld::Network => &[Self::NETWORK_IMPORT],
            FlowWorld::Storage => &[Self::STORAGE_IMPORT],
            FlowWorld::Full => &[Self::NETWORK_IMPORT, Self::STORAGE_IMPORT],
        }
    }

    fn validate_component_imports(
        actual_imports: &std::collections::BTreeSet<String>,
        declared_world: FlowWorld,
    ) -> Result<(), Status> {
        let declared_imports = Self::declared_imports_for_world(declared_world)
            .iter()
            .map(|name| name.to_string())
            .collect::<std::collections::BTreeSet<_>>();

        let unknown_imports = actual_imports
            .difference(
                &[
                    Self::NETWORK_IMPORT.to_string(),
                    Self::STORAGE_IMPORT.to_string(),
                ]
                .into_iter()
                .collect(),
            )
            .cloned()
            .collect::<Vec<_>>();
        if !unknown_imports.is_empty() {
            return Err(Status::invalid_argument(format!(
                "component imports unsupported host interfaces: {}",
                unknown_imports.join(", ")
            )));
        }

        if *actual_imports != declared_imports {
            let actual = if actual_imports.is_empty() {
                "<none>".to_string()
            } else {
                actual_imports
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let declared = if declared_imports.is_empty() {
                "<none>".to_string()
            } else {
                declared_imports
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            return Err(Status::invalid_argument(format!(
                "manifest world `{}` does not match component imports (declared={declared}, actual={actual})",
                match declared_world {
                    FlowWorld::Sandbox => "sandbox",
                    FlowWorld::Network => "network",
                    FlowWorld::Storage => "storage",
                    FlowWorld::Full => "full",
                }
            )));
        }

        Ok(())
    }

    fn validate_phase1_manifest_contract(manifest: &FlowManifest) -> Result<(), Status> {
        if manifest.id.trim().is_empty() {
            return Err(Status::invalid_argument("manifest `id` must not be empty"));
        }
        if manifest.initial_state.trim().is_empty() {
            return Err(Status::invalid_argument(
                "manifest `initial_state` must not be empty",
            ));
        }

        let mut state_names = std::collections::HashSet::new();
        for state in &manifest.states {
            if state.name.trim().is_empty() {
                return Err(Status::invalid_argument("state `name` must not be empty"));
            }
            if !state_names.insert(state.name.as_str()) {
                return Err(Status::invalid_argument(format!(
                    "duplicate state name `{}`",
                    state.name
                )));
            }
            match state.kind {
                StateKind::Fork | StateKind::Join => {
                    return Err(Status::unimplemented(format!(
                        "state kind `{}` is not implemented in Phase 1 runtime",
                        match state.kind {
                            StateKind::Fork => "fork",
                            StateKind::Join => "join",
                            _ => unreachable!(),
                        }
                    )));
                }
                StateKind::Subprocess => {
                    let subprocess = state.subprocess.as_ref().ok_or_else(|| {
                        Status::invalid_argument(format!(
                            "subprocess state `{}` must declare subprocess config",
                            state.name
                        ))
                    })?;
                    if subprocess.flow_id.trim().is_empty() {
                        return Err(Status::invalid_argument(format!(
                            "subprocess state `{}` must declare non-empty subprocess.flow_id",
                            state.name
                        )));
                    }
                    if subprocess.completion_event.trim().is_empty() {
                        return Err(Status::invalid_argument(format!(
                            "subprocess state `{}` must declare non-empty subprocess.completion_event",
                            state.name
                        )));
                    }
                    return Err(Status::unimplemented(format!(
                        "subprocess state `{}` is not implemented in Phase 1 runtime",
                        state.name
                    )));
                }
                StateKind::Normal | StateKind::Terminal => {}
            }
        }

        let mut action_names = std::collections::HashSet::new();
        for action in &manifest.actions {
            if action.name.trim().is_empty() {
                return Err(Status::invalid_argument("action `name` must not be empty"));
            }
            if !action_names.insert(action.name.as_str()) {
                return Err(Status::invalid_argument(format!(
                    "duplicate action name `{}`",
                    action.name
                )));
            }
            if let DispatchMode::FanOut(config) = &action.dispatch {
                if config.aggregator.trim().is_empty() {
                    return Err(Status::invalid_argument(format!(
                        "fan-out action `{}` must declare non-empty aggregator",
                        action.name
                    )));
                }
                if matches!(
                    config.strategy,
                    shiroha_core::flow::FanOutStrategy::Count(0)
                ) {
                    return Err(Status::invalid_argument(format!(
                        "fan-out action `{}` must use count > 0",
                        action.name
                    )));
                }
                if config.timeout_ms == Some(0) {
                    return Err(Status::invalid_argument(format!(
                        "fan-out action `{}` timeout_ms must be greater than 0",
                        action.name
                    )));
                }
                if config.min_success == Some(0) {
                    return Err(Status::invalid_argument(format!(
                        "fan-out action `{}` min_success must be greater than 0",
                        action.name
                    )));
                }
                return Err(Status::unimplemented(format!(
                    "fan-out action `{}` is not implemented in Phase 1 runtime",
                    action.name
                )));
            }
        }

        for transition in &manifest.transitions {
            if transition.from.trim().is_empty() {
                return Err(Status::invalid_argument(
                    "transition `from` must not be empty",
                ));
            }
            if transition.to.trim().is_empty() {
                return Err(Status::invalid_argument(
                    "transition `to` must not be empty",
                ));
            }
            if transition.event.trim().is_empty() {
                return Err(Status::invalid_argument(
                    "transition `event` must not be empty",
                ));
            }
            if let Some(timeout) = &transition.timeout {
                if timeout.duration_ms == 0 {
                    return Err(Status::invalid_argument(format!(
                        "transition {} --{}--> {} must use timeout.duration_ms > 0",
                        transition.from, transition.event, transition.to
                    )));
                }
                if timeout.timeout_event.trim().is_empty() {
                    return Err(Status::invalid_argument(format!(
                        "transition {} --{}--> {} must use non-empty timeout.timeout_event",
                        transition.from, transition.event, transition.to
                    )));
                }
                if timeout.timeout_event != transition.event {
                    return Err(Status::invalid_argument(format!(
                        "transition {} --{}--> {} must use timeout_event matching transition event",
                        transition.from, transition.event, transition.to
                    )));
                }
                let matching = manifest
                    .transitions
                    .iter()
                    .filter(|candidate| {
                        candidate.from == transition.from
                            && candidate.event == timeout.timeout_event
                    })
                    .count();
                if matching != 1 {
                    return Err(Status::invalid_argument(format!(
                        "timeout_event `{}` from state `{}` must resolve to exactly one transition",
                        timeout.timeout_event, transition.from
                    )));
                }
            }
        }

        Ok(())
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
        let component = self
            .state
            .wasm_runtime
            .load_component(&wasm_bytes)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        let actual_imports = component
            .component_type()
            .imports(self.state.wasm_runtime.engine())
            .map(|(name, _item)| name.to_string())
            .filter(|name| !name.starts_with("wasi:"))
            .collect::<std::collections::BTreeSet<_>>();

        let wasm_module = Arc::new(WasmModule::new(component, &wasm_bytes));

        // 从 WASM 提取 manifest
        let mut host = shiroha_wasm::host::WasmHost::new_with_capability_store(
            self.state.wasm_runtime.engine(),
            wasm_module.component(),
            self.state.storage.clone(),
        )
        .map_err(|e| Status::internal(e.to_string()))?;
        host.validate_required_exports()
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let manifest = host
            .get_manifest()
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        Self::validate_component_imports(&actual_imports, manifest.host_world)?;
        Self::validate_phase1_manifest_contract(&manifest)?;

        // 静态验证
        let warnings = FlowValidator::validate(&manifest);
        let (fatal_warnings, nonfatal_warnings): (Vec<_>, Vec<_>) = warnings
            .into_iter()
            .partition(Self::is_fatal_validation_warning);
        if !fatal_warnings.is_empty() {
            let messages = fatal_warnings
                .iter()
                .map(ValidationWarning::to_string)
                .collect::<Vec<_>>();
            return Err(Status::invalid_argument(format!(
                "flow validation failed: {}",
                messages.join("; ")
            )));
        }

        let warning_messages: Vec<String> = nonfatal_warnings
            .iter()
            .map(ValidationWarning::to_string)
            .collect();
        if !warning_messages.is_empty() {
            tracing::warn!(
                flow_id,
                warnings = ?warning_messages,
                "flow validation warnings"
            );
        }

        let version = Uuid::now_v7();
        let registration = FlowRegistration {
            // gRPC 请求里的 flow_id 是平台侧注册键；
            // manifest.id 保留 guest 自描述值，当前不强制二者一致。
            flow_id: flow_id.clone(),
            version,
            manifest: manifest.clone(),
            wasm_hash: wasm_module.hash().to_string(),
        };

        // 持久化 + 缓存
        self.state
            .storage
            .save_flow_with_wasm(&registration, &wasm_bytes)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        self.state.module_cache.insert(wasm_module);
        self.state.flow_registry.register(registration).await;

        tracing::info!(flow_id, %version, "flow deployed");

        Ok(Response::new(DeployFlowResponse {
            flow_id,
            version: version.to_string(),
            warnings: warning_messages,
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

        let summaries = flows.iter().map(Self::flow_summary).collect();

        Ok(Response::new(ListFlowsResponse { flows: summaries }))
    }

    async fn list_flow_versions(
        &self,
        request: Request<ListFlowVersionsRequest>,
    ) -> Result<Response<ListFlowVersionsResponse>, Status> {
        let flow_id = request.into_inner().flow_id;
        let flows = self
            .state
            .storage
            .list_flow_versions_for(&flow_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(ListFlowVersionsResponse {
            flows: flows.iter().map(Self::flow_summary).collect(),
        }))
    }

    async fn get_flow(
        &self,
        request: Request<GetFlowRequest>,
    ) -> Result<Response<GetFlowResponse>, Status> {
        let req = request.into_inner();
        let flow_id = req.flow_id;
        let flow = if let Some(version) = req.version {
            let version = parse_uuid(&version)?;
            self.state
                .storage
                .get_flow_version(&flow_id, version)
                .await
                .map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| {
                    Status::not_found(format!("flow `{flow_id}` version {version} not found"))
                })?
        } else {
            self.state
                .storage
                .get_flow(&flow_id)
                .await
                .map_err(|e| Status::internal(e.to_string()))?
                .ok_or_else(|| Status::not_found(format!("flow `{flow_id}` not found")))?
        };

        let manifest_json =
            serde_json::to_string(&flow.manifest).map_err(|e| Status::internal(e.to_string()))?;

        Ok(Response::new(GetFlowResponse {
            flow_id: flow.flow_id,
            version: flow.version.to_string(),
            manifest_json,
        }))
    }

    async fn delete_flow(
        &self,
        request: Request<DeleteFlowRequest>,
    ) -> Result<Response<DeleteFlowResponse>, Status> {
        let flow_id = request.into_inner().flow_id;
        let flow = self
            .state
            .storage
            .get_flow(&flow_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::not_found(format!("flow `{flow_id}` not found")))?;
        let jobs = self
            .state
            .job_manager
            .list_jobs(&flow_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        if !jobs.is_empty() {
            return Err(Status::failed_precondition(format!(
                "flow `{flow_id}` still has {} job(s)",
                jobs.len()
            )));
        }

        self.state
            .storage
            .delete_flow(&flow_id)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;

        self.state.flow_registry.remove_flow(&flow_id).await;

        tracing::info!(flow_id, version = %flow.version, "flow deleted");

        Ok(Response::new(DeleteFlowResponse { flow_id }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_service::JobServiceImpl;
    use crate::test_support::{
        TestHarness, approval_manifest, register_flow_version, warning_manifest, wasm_for_manifest,
    };
    use shiroha_core::flow::{
        ActionDef, DispatchMode, FanOutConfig, FanOutStrategy, FlowManifest, FlowWorld, StateDef,
        StateKind, TimeoutDef, TransitionDef,
    };
    use shiroha_core::storage::Storage;
    use shiroha_proto::shiroha_api::job_service_server::JobService;
    use shiroha_proto::shiroha_api::{
        CreateJobRequest, DeleteFlowRequest, GetFlowRequest, ListFlowVersionsRequest,
        ListFlowsRequest,
    };

    fn invalid_reference_manifest() -> FlowManifest {
        FlowManifest {
            id: "invalid-reference".into(),
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
                event: "approve".into(),
                guard: Some("allow".into()),
                action: Some("ship".into()),
                timeout: None,
            }],
            initial_state: "idle".into(),
            actions: Vec::new(),
        }
    }

    fn unsupported_fanout_manifest() -> FlowManifest {
        FlowManifest {
            id: "fanout-demo".into(),
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
                event: "collect".into(),
                guard: None,
                action: Some("collect".into()),
                timeout: None,
            }],
            initial_state: "idle".into(),
            actions: vec![ActionDef {
                name: "collect".into(),
                dispatch: DispatchMode::FanOut(FanOutConfig {
                    strategy: FanOutStrategy::Count(2),
                    aggregator: "pick-success".into(),
                    timeout_ms: Some(100),
                    min_success: Some(1),
                }),
                capabilities: Vec::new(),
            }],
        }
    }

    fn invalid_timeout_manifest() -> FlowManifest {
        FlowManifest {
            id: "timeout-invalid".into(),
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
                event: "approve".into(),
                guard: None,
                action: None,
                timeout: Some(TimeoutDef {
                    duration_ms: 10,
                    timeout_event: "expire".into(),
                }),
            }],
            initial_state: "idle".into(),
            actions: Vec::new(),
        }
    }

    fn invalid_subprocess_manifest() -> FlowManifest {
        FlowManifest {
            id: "subprocess-invalid".into(),
            host_world: FlowWorld::Sandbox,
            states: vec![StateDef {
                name: "legal-review".into(),
                kind: StateKind::Subprocess,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            }],
            transitions: Vec::new(),
            initial_state: "legal-review".into(),
            actions: Vec::new(),
        }
    }

    #[tokio::test]
    #[ignore = "heavy service integration smoke; run explicitly when validating deploy/query flows"]
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
                version: None,
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

    #[tokio::test]
    #[ignore = "heavy service integration smoke; run explicitly when validating deploy/query flows"]
    async fn deploy_flow_returns_validation_warnings() {
        let harness = TestHarness::new("flow-warnings").await;
        let service = FlowServiceImpl::new(harness.state.clone());

        let deploy = service
            .deploy_flow(Request::new(DeployFlowRequest {
                flow_id: "warning-demo".into(),
                wasm_bytes: wasm_for_manifest(&warning_manifest()),
            }))
            .await
            .expect("deploy flow")
            .into_inner();

        assert!(!deploy.warnings.is_empty());
        assert!(
            deploy
                .warnings
                .iter()
                .any(|warning| warning.contains("cannot reach any terminal state"))
        );
    }

    #[tokio::test]
    #[ignore = "heavy service integration smoke; run explicitly when validating deploy/query flows"]
    async fn deploy_flow_rejects_fatal_validation_issues() {
        let harness = TestHarness::new("flow-fatal-validation").await;
        let service = FlowServiceImpl::new(harness.state.clone());

        let error = service
            .deploy_flow(Request::new(DeployFlowRequest {
                flow_id: "invalid-reference".into(),
                wasm_bytes: wasm_for_manifest(&invalid_reference_manifest()),
            }))
            .await
            .expect_err("fatal validation issues should fail deploy");

        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert!(error.message().contains("action `ship`"));
        assert!(error.message().contains("guard `allow`"));
    }

    #[tokio::test]
    #[ignore = "heavy service integration smoke; run explicitly when validating deploy/query flows"]
    async fn deploy_flow_rejects_phase1_unsupported_fanout() {
        let harness = TestHarness::new("flow-fanout-unsupported").await;
        let service = FlowServiceImpl::new(harness.state.clone());

        let error = service
            .deploy_flow(Request::new(DeployFlowRequest {
                flow_id: "fanout-demo".into(),
                wasm_bytes: wasm_for_manifest(&unsupported_fanout_manifest()),
            }))
            .await
            .expect_err("fanout should fail in phase1");

        assert_eq!(error.code(), tonic::Code::Unimplemented);
        assert!(error.message().contains("fan-out action `collect`"));
    }

    #[tokio::test]
    #[ignore = "heavy service integration smoke; run explicitly when validating deploy/query flows"]
    async fn deploy_flow_rejects_timeout_event_mismatch() {
        let harness = TestHarness::new("flow-timeout-invalid").await;
        let service = FlowServiceImpl::new(harness.state.clone());

        let error = service
            .deploy_flow(Request::new(DeployFlowRequest {
                flow_id: "timeout-invalid".into(),
                wasm_bytes: wasm_for_manifest(&invalid_timeout_manifest()),
            }))
            .await
            .expect_err("timeout mismatch should fail");

        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert!(
            error
                .message()
                .contains("timeout_event matching transition event")
        );
    }

    #[tokio::test]
    #[ignore = "heavy service integration smoke; run explicitly when validating deploy/query flows"]
    async fn deploy_flow_rejects_subprocess_without_config() {
        let harness = TestHarness::new("flow-subprocess-invalid").await;
        let service = FlowServiceImpl::new(harness.state.clone());

        let error = service
            .deploy_flow(Request::new(DeployFlowRequest {
                flow_id: "subprocess-invalid".into(),
                wasm_bytes: wasm_for_manifest(&invalid_subprocess_manifest()),
            }))
            .await
            .expect_err("subprocess config should fail");

        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert!(error.message().contains("must declare subprocess config"));
    }

    #[test]
    fn validate_component_imports_rejects_manifest_world_mismatch() {
        let actual_imports = std::collections::BTreeSet::new();

        let error =
            FlowServiceImpl::validate_component_imports(&actual_imports, FlowWorld::Network)
                .expect_err("world mismatch should fail");

        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert!(error.message().contains("manifest world `network`"));
    }

    #[tokio::test]
    async fn list_flow_versions_and_get_specific_version_work() {
        let harness = TestHarness::new("flow-service-version-query").await;
        let service = FlowServiceImpl::new(harness.state.clone());
        let first = approval_manifest("demo-flow", Some("allow"));
        let second = approval_manifest("demo-flow", Some("deny"));

        let first_deploy =
            register_flow_version(&harness.state, "demo-flow", Uuid::now_v7(), first).await;
        let second_deploy =
            register_flow_version(&harness.state, "demo-flow", Uuid::now_v7(), second).await;

        let latest = service
            .get_flow(Request::new(GetFlowRequest {
                flow_id: "demo-flow".into(),
                version: None,
            }))
            .await
            .expect("get latest flow")
            .into_inner();
        assert_eq!(latest.version, second_deploy.version.to_string());

        let first_version = service
            .get_flow(Request::new(GetFlowRequest {
                flow_id: "demo-flow".into(),
                version: Some(first_deploy.version.to_string()),
            }))
            .await
            .expect("get first version")
            .into_inner();
        assert_eq!(first_version.version, first_deploy.version.to_string());

        let versions = service
            .list_flow_versions(Request::new(ListFlowVersionsRequest {
                flow_id: "demo-flow".into(),
            }))
            .await
            .expect("list flow versions")
            .into_inner();
        assert_eq!(versions.flows.len(), 2);
        assert!(
            versions
                .flows
                .iter()
                .any(|flow| flow.version == first_deploy.version.to_string())
        );
        assert!(
            versions
                .flows
                .iter()
                .any(|flow| flow.version == second_deploy.version.to_string())
        );

        let latest_list = service
            .list_flows(Request::new(ListFlowsRequest {}))
            .await
            .expect("list latest flows")
            .into_inner();
        assert_eq!(latest_list.flows.len(), 1);
        assert_eq!(
            latest_list.flows[0].version,
            second_deploy.version.to_string()
        );
    }

    #[tokio::test]
    #[ignore = "heavy service integration smoke; run explicitly when validating deploy/query flows"]
    async fn delete_flow_removes_storage_and_memory_cache() {
        let harness = TestHarness::new("flow-delete").await;
        let service = FlowServiceImpl::new(harness.state.clone());
        let manifest = approval_manifest("demo-flow", Some("allow"));

        service
            .deploy_flow(Request::new(DeployFlowRequest {
                flow_id: "demo-flow".into(),
                wasm_bytes: wasm_for_manifest(&manifest),
            }))
            .await
            .expect("deploy flow");

        service
            .delete_flow(Request::new(DeleteFlowRequest {
                flow_id: "demo-flow".into(),
            }))
            .await
            .expect("delete flow");

        assert!(
            harness
                .state
                .storage
                .get_flow("demo-flow")
                .await
                .expect("get flow")
                .is_none()
        );
        assert!(
            harness
                .state
                .flow_registry
                .latest_registration("demo-flow")
                .await
                .is_none()
        );
        assert!(
            harness
                .state
                .flow_registry
                .latest_engine("demo-flow")
                .await
                .is_none()
        );
    }

    #[tokio::test]
    #[ignore = "heavy service integration smoke; run explicitly when validating deploy/query flows"]
    async fn delete_flow_rejects_when_jobs_still_exist() {
        let harness = TestHarness::new("flow-delete-jobs").await;
        let flow_service = FlowServiceImpl::new(harness.state.clone());
        let job_service = JobServiceImpl::new(harness.state.clone());
        let manifest = approval_manifest("demo-flow", Some("allow"));

        flow_service
            .deploy_flow(Request::new(DeployFlowRequest {
                flow_id: "demo-flow".into(),
                wasm_bytes: wasm_for_manifest(&manifest),
            }))
            .await
            .expect("deploy flow");
        job_service
            .create_job(Request::new(CreateJobRequest {
                flow_id: "demo-flow".into(),
                context: None,
                max_lifetime_ms: None,
            }))
            .await
            .expect("create job");

        let error = flow_service
            .delete_flow(Request::new(DeleteFlowRequest {
                flow_id: "demo-flow".into(),
            }))
            .await
            .expect_err("delete flow with jobs");

        assert_eq!(error.code(), tonic::Code::FailedPrecondition);
    }
}
