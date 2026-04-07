//! WASM Host-Guest 桥接层
//!
//! [`WasmHost`] 仅支持 component/wasip2 guest。
//! guest 需按 canonical `shiroha-wit` world 导出 typed exports，host 通过
//! `wasmtime::component::Instance::get_typed_func` 调用。

mod network_support;
mod storage_support;

use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};
use wasmtime::component::{ComponentNamedList, ComponentType, Lift, Lower, ResourceTable};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

use shiroha_core::flow::{
    ActionCapability, ActionDef, DispatchMode, FanOutConfig, FanOutStrategy, FlowManifest,
    FlowWorld, StateDef, StateKind, SubprocessDef, TimeoutDef, TransitionDef,
};
use shiroha_core::job::{ActionResult, AggregateDecision, ExecutionStatus, NodeResult};
use shiroha_core::storage::{CapabilityStore, MemoryStorage};

use crate::error::WasmError;

const DEFAULT_FUEL: u64 = 1_000_000;
// 兼容 WIT 生成代码中常见的 kebab-case / snake_case 两种导出命名。
const GET_MANIFEST_EXPORTS: &[&str] = &["get-manifest", "get_manifest"];
const INVOKE_ACTION_EXPORTS: &[&str] = &["invoke-action", "invoke_action"];
const INVOKE_GUARD_EXPORTS: &[&str] = &["invoke-guard", "invoke_guard"];
const AGGREGATE_EXPORTS: &[&str] = &["aggregate"];

/// Action 执行上下文，传入 WASM guest
#[derive(Debug, Clone, Serialize, Deserialize, ComponentType, Lift, Lower)]
#[component(record)]
pub struct ActionContext {
    #[component(name = "job-id")]
    pub job_id: String,
    pub state: String,
    pub context: Option<Vec<u8>>,
    pub payload: Option<Vec<u8>>,
}

/// Guard 评估上下文，传入 WASM guest
#[derive(Debug, Clone, Serialize, Deserialize, ComponentType, Lift, Lower)]
#[component(record)]
pub struct GuardContext {
    #[component(name = "job-id")]
    pub job_id: String,
    #[component(name = "from-state")]
    pub from_state: String,
    #[component(name = "to-state")]
    pub to_state: String,
    pub event: String,
    pub context: Option<Vec<u8>>,
    pub payload: Option<Vec<u8>>,
}

struct ComponentStoreState {
    // 当前 host 只提供最小 WASI 上下文，没有额外的业务态共享给 guest。
    ctx: WasiCtx,
    table: ResourceTable,
    capability_store: Arc<dyn CapabilityStore>,
    allow_network: bool,
    allow_storage: bool,
}

impl Default for ComponentStoreState {
    fn default() -> Self {
        Self {
            ctx: WasiCtx::default(),
            table: ResourceTable::new(),
            capability_store: default_capability_store(),
            allow_network: false,
            allow_storage: false,
        }
    }
}

fn default_capability_store() -> Arc<dyn CapabilityStore> {
    static STORE: OnceLock<Arc<MemoryStorage>> = OnceLock::new();
    STORE.get_or_init(|| Arc::new(MemoryStorage::new())).clone()
}

impl WasiView for ComponentStoreState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

struct ComponentGuest {
    store: wasmtime::Store<ComponentStoreState>,
    instance: wasmtime::component::Instance,
}

impl ComponentGuest {
    fn new(
        engine: &wasmtime::Engine,
        component: &wasmtime::component::Component,
        capability_store: Arc<dyn CapabilityStore>,
    ) -> Result<Self, WasmError> {
        let mut linker = wasmtime::component::Linker::new(engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
            .map_err(|e| WasmError::Instantiation(e.to_string()))?;
        network_support::add_to_linker(&mut linker)?;
        storage_support::add_to_linker(&mut linker)?;

        // 每次调用都实例化独立的 store/component instance，避免 fuel 计数、
        // guest 内部状态和资源句柄在不同请求之间相互污染。
        let mut store = wasmtime::Store::new(
            engine,
            ComponentStoreState {
                capability_store,
                ..ComponentStoreState::default()
            },
        );
        store
            .set_fuel(DEFAULT_FUEL)
            .map_err(|e| WasmError::Instantiation(e.to_string()))?;

        let instance = linker
            .instantiate(&mut store, component)
            .map_err(|e| WasmError::Instantiation(e.to_string()))?;

        Ok(Self { store, instance })
    }

    fn get_typed_func<Params, Results>(
        &mut self,
        export_names: &[&str],
    ) -> Result<wasmtime::component::TypedFunc<Params, Results>, WasmError>
    where
        Params: ComponentNamedList + Lower,
        Results: ComponentNamedList + Lift,
    {
        // 允许 guest 端导出名在两种命名风格之间切换，而不影响 host 侧调用代码。
        for &name in export_names {
            if let Ok(func) = self
                .instance
                .get_typed_func::<Params, Results>(&mut self.store, name)
            {
                return Ok(func);
            }
        }

        Err(WasmError::Instantiation(format!(
            "missing component export: one of {}",
            export_names.join(", ")
        )))
    }
}

// 下面这组 `Component*` 类型精确镜像 WIT ABI 形状。
// `WasmHost` 先通过它们与 component 交互，再统一转换成 `shiroha_core`
// 中的领域类型，避免把 wasmtime 派生宏泄漏到其他 crate。
#[derive(Debug, Clone, ComponentType, Lift, Lower)]
#[component(record)]
struct ComponentFlowManifest {
    id: String,
    #[component(name = "host-world")]
    world: ComponentFlowWorld,
    states: Vec<ComponentStateDef>,
    transitions: Vec<ComponentTransitionDef>,
    #[component(name = "initial-state")]
    initial_state: String,
    actions: Vec<ComponentActionDef>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, ComponentType, Lift, Lower)]
#[component(enum)]
#[repr(u8)]
enum ComponentFlowWorld {
    #[component(name = "sandbox")]
    Sandbox,
    #[component(name = "network")]
    Network,
    #[component(name = "storage")]
    Storage,
    #[component(name = "full")]
    Full,
}

#[derive(Debug, Clone, ComponentType, Lift, Lower)]
#[component(record)]
struct ComponentStateDef {
    name: String,
    kind: ComponentStateKind,
    #[component(name = "on-enter")]
    on_enter: Option<String>,
    #[component(name = "on-exit")]
    on_exit: Option<String>,
    subprocess: Option<ComponentSubprocessDef>,
}

#[derive(Debug, Clone, ComponentType, Lift, Lower)]
#[component(variant)]
enum ComponentStateKind {
    #[component(name = "normal")]
    Normal,
    #[component(name = "terminal")]
    Terminal,
    #[component(name = "fork")]
    Fork,
    #[component(name = "join")]
    Join,
    #[component(name = "subprocess")]
    Subprocess,
}

#[derive(Debug, Clone, ComponentType, Lift, Lower)]
#[component(record)]
struct ComponentSubprocessDef {
    #[component(name = "flow-id")]
    flow_id: String,
    #[component(name = "completion-event")]
    completion_event: String,
}

#[derive(Debug, Clone, ComponentType, Lift, Lower)]
#[component(record)]
struct ComponentTransitionDef {
    from: String,
    to: String,
    event: String,
    guard: Option<String>,
    action: Option<String>,
    timeout: Option<ComponentTimeoutDef>,
}

#[derive(Debug, Clone, ComponentType, Lift, Lower)]
#[component(record)]
struct ComponentTimeoutDef {
    #[component(name = "duration-ms")]
    duration_ms: u64,
    #[component(name = "timeout-event")]
    timeout_event: String,
}

#[derive(Debug, Clone, ComponentType, Lift, Lower)]
#[component(record)]
struct ComponentActionDef {
    name: String,
    dispatch: ComponentDispatchMode,
    capabilities: Vec<ComponentActionCapability>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, ComponentType, Lift, Lower)]
#[component(enum)]
#[repr(u8)]
enum ComponentActionCapability {
    #[component(name = "network")]
    Network,
    #[component(name = "storage")]
    Storage,
}

#[derive(Debug, Clone, ComponentType, Lift, Lower)]
#[component(variant)]
enum ComponentDispatchMode {
    #[component(name = "local")]
    Local,
    #[component(name = "remote")]
    Remote,
    #[component(name = "fan-out")]
    FanOut(ComponentFanOutConfig),
}

#[derive(Debug, Clone, ComponentType, Lift, Lower)]
#[component(record)]
struct ComponentFanOutConfig {
    strategy: ComponentFanOutStrategy,
    aggregator: String,
    #[component(name = "timeout-ms")]
    timeout_ms: Option<u64>,
    #[component(name = "min-success")]
    min_success: Option<u32>,
}

#[derive(Debug, Clone, ComponentType, Lift, Lower)]
#[component(variant)]
enum ComponentFanOutStrategy {
    #[component(name = "all")]
    All,
    #[component(name = "count")]
    Count(u32),
    #[component(name = "tagged")]
    Tagged(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ComponentType, Lift, Lower)]
#[component(enum)]
#[repr(u8)]
enum ComponentExecutionStatus {
    #[component(name = "success")]
    Success,
    #[component(name = "failed")]
    Failed,
    #[component(name = "timeout")]
    Timeout,
}

#[derive(Debug, Clone, ComponentType, Lift, Lower)]
#[component(record)]
struct ComponentActionResult {
    status: ComponentExecutionStatus,
    output: Option<Vec<u8>>,
}

#[derive(Debug, Clone, ComponentType, Lift, Lower)]
#[component(record)]
struct ComponentNodeResult {
    #[component(name = "node-id")]
    node_id: String,
    status: ComponentExecutionStatus,
    output: Option<Vec<u8>>,
}

#[derive(Debug, Clone, ComponentType, Lift, Lower)]
#[component(record)]
struct ComponentAggregateDecision {
    event: String,
    #[component(name = "context-patch")]
    context_patch: Option<Vec<u8>>,
}

impl From<ComponentFlowManifest> for FlowManifest {
    fn from(value: ComponentFlowManifest) -> Self {
        Self {
            id: value.id,
            host_world: value.world.into(),
            states: value.states.into_iter().map(Into::into).collect(),
            transitions: value.transitions.into_iter().map(Into::into).collect(),
            initial_state: value.initial_state,
            actions: value.actions.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ComponentFlowWorld> for FlowWorld {
    fn from(value: ComponentFlowWorld) -> Self {
        match value {
            ComponentFlowWorld::Sandbox => Self::Sandbox,
            ComponentFlowWorld::Network => Self::Network,
            ComponentFlowWorld::Storage => Self::Storage,
            ComponentFlowWorld::Full => Self::Full,
        }
    }
}

impl From<ComponentStateDef> for StateDef {
    fn from(value: ComponentStateDef) -> Self {
        Self {
            name: value.name,
            kind: value.kind.into(),
            on_enter: value.on_enter,
            on_exit: value.on_exit,
            subprocess: value.subprocess.map(Into::into),
        }
    }
}

impl From<ComponentStateKind> for StateKind {
    fn from(value: ComponentStateKind) -> Self {
        match value {
            ComponentStateKind::Normal => Self::Normal,
            ComponentStateKind::Terminal => Self::Terminal,
            ComponentStateKind::Fork => Self::Fork,
            ComponentStateKind::Join => Self::Join,
            ComponentStateKind::Subprocess => Self::Subprocess,
        }
    }
}

impl From<ComponentSubprocessDef> for SubprocessDef {
    fn from(value: ComponentSubprocessDef) -> Self {
        Self {
            flow_id: value.flow_id,
            completion_event: value.completion_event,
        }
    }
}

impl From<ComponentTransitionDef> for TransitionDef {
    fn from(value: ComponentTransitionDef) -> Self {
        Self {
            from: value.from,
            to: value.to,
            event: value.event,
            guard: value.guard,
            action: value.action,
            timeout: value.timeout.map(Into::into),
        }
    }
}

impl From<ComponentTimeoutDef> for TimeoutDef {
    fn from(value: ComponentTimeoutDef) -> Self {
        Self {
            duration_ms: value.duration_ms,
            timeout_event: value.timeout_event,
        }
    }
}

impl From<ComponentActionDef> for ActionDef {
    fn from(value: ComponentActionDef) -> Self {
        Self {
            name: value.name,
            dispatch: value.dispatch.into(),
            capabilities: value.capabilities.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ComponentActionCapability> for ActionCapability {
    fn from(value: ComponentActionCapability) -> Self {
        match value {
            ComponentActionCapability::Network => Self::Network,
            ComponentActionCapability::Storage => Self::Storage,
        }
    }
}

impl From<ComponentDispatchMode> for DispatchMode {
    fn from(value: ComponentDispatchMode) -> Self {
        match value {
            ComponentDispatchMode::Local => Self::Local,
            ComponentDispatchMode::Remote => Self::Remote,
            ComponentDispatchMode::FanOut(config) => Self::FanOut(config.into()),
        }
    }
}

impl From<ComponentFanOutConfig> for FanOutConfig {
    fn from(value: ComponentFanOutConfig) -> Self {
        Self {
            strategy: value.strategy.into(),
            aggregator: value.aggregator,
            timeout_ms: value.timeout_ms,
            min_success: value.min_success,
        }
    }
}

impl From<ComponentFanOutStrategy> for FanOutStrategy {
    fn from(value: ComponentFanOutStrategy) -> Self {
        match value {
            ComponentFanOutStrategy::All => Self::All,
            ComponentFanOutStrategy::Count(count) => Self::Count(count),
            ComponentFanOutStrategy::Tagged(tags) => Self::Tagged(tags),
        }
    }
}

impl From<ComponentExecutionStatus> for ExecutionStatus {
    fn from(value: ComponentExecutionStatus) -> Self {
        match value {
            ComponentExecutionStatus::Success => Self::Success,
            ComponentExecutionStatus::Failed => Self::Failed,
            ComponentExecutionStatus::Timeout => Self::Timeout,
        }
    }
}

impl From<ComponentActionResult> for ActionResult {
    fn from(value: ComponentActionResult) -> Self {
        Self {
            status: value.status.into(),
            output: value.output,
        }
    }
}

impl From<ExecutionStatus> for ComponentExecutionStatus {
    fn from(value: ExecutionStatus) -> Self {
        match value {
            ExecutionStatus::Success => Self::Success,
            ExecutionStatus::Failed => Self::Failed,
            ExecutionStatus::Timeout => Self::Timeout,
        }
    }
}

impl From<&NodeResult> for ComponentNodeResult {
    fn from(value: &NodeResult) -> Self {
        Self {
            node_id: value.node_id.clone(),
            status: value.status.into(),
            output: value.output.clone(),
        }
    }
}

impl From<ComponentAggregateDecision> for AggregateDecision {
    fn from(value: ComponentAggregateDecision) -> Self {
        Self {
            event: value.event,
            context_patch: value.context_patch,
        }
    }
}

/// WASM component 的 host 端代理
///
/// 该类型本身只持有可复用的编译产物；真正的 guest 实例会在每次方法调用时创建。
pub struct WasmHost {
    engine: wasmtime::Engine,
    component: wasmtime::component::Component,
    capability_store: Arc<dyn CapabilityStore>,
}

impl WasmHost {
    pub fn new(
        engine: &wasmtime::Engine,
        component: &wasmtime::component::Component,
    ) -> Result<Self, WasmError> {
        Self::new_with_capability_store(engine, component, default_capability_store())
    }

    pub fn new_with_capability_store(
        engine: &wasmtime::Engine,
        component: &wasmtime::component::Component,
        capability_store: Arc<dyn CapabilityStore>,
    ) -> Result<Self, WasmError> {
        Ok(Self {
            engine: engine.clone(),
            component: component.clone(),
            capability_store,
        })
    }

    /// 创建一次性 guest 实例，用于单次 typed export 调用。
    fn guest(&self) -> Result<ComponentGuest, WasmError> {
        ComponentGuest::new(&self.engine, &self.component, self.capability_store.clone())
    }

    fn set_allowed_capabilities(guest: &mut ComponentGuest, capabilities: &[ActionCapability]) {
        guest.store.data_mut().allow_network = false;
        guest.store.data_mut().allow_storage = false;
        for capability in capabilities {
            match capability {
                ActionCapability::Network => guest.store.data_mut().allow_network = true,
                ActionCapability::Storage => guest.store.data_mut().allow_storage = true,
            }
        }
    }

    pub fn validate_required_exports(&self) -> Result<(), WasmError> {
        let mut guest = self.guest()?;
        let _ = guest.get_typed_func::<(), (ComponentFlowManifest,)>(GET_MANIFEST_EXPORTS)?;
        let _ = guest.get_typed_func::<(String, ActionContext), (ComponentActionResult,)>(
            INVOKE_ACTION_EXPORTS,
        )?;
        let _ = guest.get_typed_func::<(String, GuardContext), (bool,)>(INVOKE_GUARD_EXPORTS)?;
        let _ = guest
            .get_typed_func::<(String, Vec<ComponentNodeResult>), (ComponentAggregateDecision,)>(
                AGGREGATE_EXPORTS,
            )?;
        Ok(())
    }

    pub fn get_manifest(&mut self) -> Result<FlowManifest, WasmError> {
        let mut guest = self.guest()?;
        Self::set_allowed_capabilities(&mut guest, &[]);
        let get_manifest =
            guest.get_typed_func::<(), (ComponentFlowManifest,)>(GET_MANIFEST_EXPORTS)?;
        let (manifest,) = get_manifest
            .call(&mut guest.store, ())
            .map_err(|e| WasmError::Execution(e.to_string()))?;
        Ok(manifest.into())
    }

    pub fn invoke_action(
        &mut self,
        name: &str,
        ctx: ActionContext,
        capabilities: &[ActionCapability],
    ) -> Result<ActionResult, WasmError> {
        let mut guest = self.guest()?;
        Self::set_allowed_capabilities(&mut guest, capabilities);
        let invoke_action = guest
            .get_typed_func::<(String, ActionContext), (ComponentActionResult,)>(
                INVOKE_ACTION_EXPORTS,
            )?;
        let (result,) = invoke_action
            .call(&mut guest.store, (name.to_string(), ctx))
            .map_err(|e| WasmError::Execution(e.to_string()))?;
        Ok(result.into())
    }

    pub fn invoke_guard(&mut self, name: &str, ctx: GuardContext) -> Result<bool, WasmError> {
        let mut guest = self.guest()?;
        Self::set_allowed_capabilities(&mut guest, &[]);
        let invoke_guard =
            guest.get_typed_func::<(String, GuardContext), (bool,)>(INVOKE_GUARD_EXPORTS)?;
        let (accepted,) = invoke_guard
            .call(&mut guest.store, (name.to_string(), ctx))
            .map_err(|e| WasmError::Execution(e.to_string()))?;
        Ok(accepted)
    }

    pub fn aggregate(
        &mut self,
        name: &str,
        results: &[NodeResult],
    ) -> Result<AggregateDecision, WasmError> {
        // fan-out 聚合要先把领域层结果重新编码为 WIT 记录数组，再交给 guest 聚合函数。
        let mut guest = self.guest()?;
        Self::set_allowed_capabilities(&mut guest, &[]);
        let typed_results: Vec<ComponentNodeResult> =
            results.iter().map(ComponentNodeResult::from).collect();
        let aggregate = guest
            .get_typed_func::<(String, Vec<ComponentNodeResult>), (ComponentAggregateDecision,)>(
                AGGREGATE_EXPORTS,
            )?;
        let (decision,) = aggregate
            .call(&mut guest.store, (name.to_string(), typed_results))
            .map_err(|e| WasmError::Execution(e.to_string()))?;
        Ok(decision.into())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::fs;
    use std::hash::{Hash, Hasher};
    use std::path::Path;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::{Mutex as StdMutex, OnceLock};

    use super::*;
    use crate::runtime::WasmRuntime;

    const CANONICAL_WIT_FILES: &[&str] = &[
        "flow.wit",
        "net.wit",
        "store.wit",
        "network-flow.wit",
        "storage-flow.wit",
        "full-flow.wit",
    ];

    fn hash_file_if_present(path: &Path, hasher: &mut DefaultHasher) {
        path.hash(hasher);
        if let Ok(bytes) = fs::read(path) {
            bytes.hash(hasher);
        }
    }

    fn tracked_fixture_input_paths(root: &Path, fixture_path: &str) -> Vec<PathBuf> {
        let mut paths = vec![
            root.join(fixture_path).join("Cargo.toml"),
            root.join(fixture_path).join("src/lib.rs"),
            root.join("../shiroha-sdk/Cargo.toml"),
            root.join("../shiroha-sdk/build.rs"),
            root.join("../shiroha-sdk/src/lib.rs"),
            root.join("../shiroha-wit/Cargo.toml"),
            root.join("../shiroha-wit/src/lib.rs"),
        ];

        paths.extend(
            CANONICAL_WIT_FILES
                .iter()
                .map(|file| root.join("../shiroha-wit/wit").join(file)),
        );
        paths
    }

    fn fixture_build_key(root: &Path, fixture_path: &str, extra_key: &str) -> String {
        let mut hasher = DefaultHasher::new();
        extra_key.hash(&mut hasher);
        for path in tracked_fixture_input_paths(root, fixture_path) {
            hash_file_if_present(&path, &mut hasher);
        }
        format!("{:016x}", hasher.finish())
    }

    fn fixture_target_dir(root: &Path, fixture_path: &str, extra_key: &str) -> PathBuf {
        let fixture_key = fixture_path.replace('/', "-");
        std::env::temp_dir()
            .join("shiroha-wasm-fixtures")
            .join(fixture_key)
            .join(fixture_build_key(root, fixture_path, extra_key))
    }

    fn build_fixture(
        fixture_path: &str,
        package_name: &str,
        extra_env: Option<(&str, &str)>,
    ) -> Vec<u8> {
        static BUILD_LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
        let _guard = BUILD_LOCK
            .get_or_init(|| StdMutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let manifest = root.join(fixture_path).join("Cargo.toml");
        let extra_key = extra_env
            .map(|(name, value)| format!("{name}={value}"))
            .unwrap_or_default();
        let target_dir = fixture_target_dir(&root, fixture_path, &extra_key);
        let wasm_path = target_dir
            .join("wasm32-wasip2")
            .join("release")
            .join(format!("{package_name}{}", std::env::consts::EXE_SUFFIX))
            .with_extension("wasm");

        if !wasm_path.exists() {
            let mut command = Command::new("cargo");
            command
                .arg("build")
                .arg("--manifest-path")
                .arg(&manifest)
                .arg("--offline")
                .arg("--target")
                .arg("wasm32-wasip2")
                .arg("--release")
                .env("CARGO_TARGET_DIR", &target_dir)
                .current_dir(&root);
            if let Some((name, value)) = extra_env {
                command.env(name, value);
            }
            let status = command.status().expect("build fixture");
            assert!(status.success(), "fixture build failed");
        }

        fs::read(&wasm_path).expect("read built fixture component")
    }

    fn build_network_fixture(url: &str) -> Vec<u8> {
        build_fixture(
            "test-fixtures/network-component",
            "network_component_fixture",
            Some(("SHIROHA_NETWORK_URL", url)),
        )
    }

    fn build_storage_fixture() -> Vec<u8> {
        build_fixture(
            "test-fixtures/storage-component",
            "storage_component_fixture",
            None,
        )
    }

    #[test]
    fn validate_required_exports_rejects_components_without_flow_world_exports() {
        let runtime = WasmRuntime::new().expect("runtime");
        let component = runtime
            .load_component(b"(component)")
            .expect("component should compile");
        let host = WasmHost::new(runtime.engine(), &component).expect("host");

        let error = host
            .validate_required_exports()
            .expect_err("missing required exports should fail");

        assert!(matches!(error, WasmError::Instantiation(_)));
    }

    #[test]
    #[ignore = "heavy capability fixture smoke; run explicitly when validating wasm host imports"]
    fn network_fixture_respects_capability_gating() {
        let runtime = WasmRuntime::new().expect("runtime");
        let wasm_bytes = build_network_fixture("http://127.0.0.1:1/");
        let component = runtime
            .load_component(&wasm_bytes)
            .expect("network fixture should compile");
        let mut host = WasmHost::new(runtime.engine(), &component).expect("host");

        host.validate_required_exports()
            .expect("network fixture should satisfy exports");
        let result = host
            .invoke_action(
                "fetch",
                ActionContext {
                    job_id: "job-1".into(),
                    state: "idle".into(),
                    context: None,
                    payload: None,
                },
                &[ActionCapability::Network],
            )
            .expect("invoke action");

        assert_eq!(result.status, ExecutionStatus::Failed);
        let output = String::from_utf8(result.output.expect("output")).expect("utf-8 output");
        assert!(output.contains("network error:"));

        let error = host
            .invoke_action(
                "fetch",
                ActionContext {
                    job_id: "job-1".into(),
                    state: "idle".into(),
                    context: None,
                    payload: None,
                },
                &[],
            )
            .expect_err("network import should be rejected without capability");

        assert!(matches!(error, WasmError::Execution(_)));
    }

    #[test]
    #[ignore = "heavy capability fixture smoke; run explicitly when validating wasm host imports"]
    fn storage_fixture_respects_capability_gating() {
        let runtime = WasmRuntime::new().expect("runtime");
        let wasm_bytes = build_storage_fixture();
        let component = runtime
            .load_component(&wasm_bytes)
            .expect("storage fixture should compile");
        let mut host = WasmHost::new(runtime.engine(), &component).expect("host");

        host.validate_required_exports()
            .expect("storage fixture should satisfy exports");
        let result = host
            .invoke_action(
                "store",
                ActionContext {
                    job_id: "job-1".into(),
                    state: "idle".into(),
                    context: None,
                    payload: None,
                },
                &[ActionCapability::Storage],
            )
            .expect("invoke action");

        assert_eq!(result.status, ExecutionStatus::Success);
        let output = String::from_utf8(result.output.expect("output")).expect("utf-8 output");
        assert!(output.contains("alpha=one"));
        assert!(output.contains("beta"));
        assert!(output.contains("deleted=true"));
        assert!(output.contains("alpha_after_delete=false"));

        let error = host
            .invoke_action(
                "store",
                ActionContext {
                    job_id: "job-1".into(),
                    state: "idle".into(),
                    context: None,
                    payload: None,
                },
                &[],
            )
            .expect_err("storage import should be rejected without capability");

        assert!(matches!(error, WasmError::Execution(_)));
    }
}
