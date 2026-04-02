use serde::{Deserialize, Serialize};

use shiroha_core::flow::FlowManifest;
use shiroha_core::job::{ActionResult, AggregateDecision, NodeResult};

use crate::error::WasmError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionContext {
    pub job_id: String,
    pub state: String,
    pub payload: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardContext {
    pub job_id: String,
    pub from_state: String,
    pub to_state: String,
    pub event: String,
    pub payload: Option<Vec<u8>>,
}

pub struct WasmHost {
    _engine: wasmtime::Engine,
    _module: wasmtime::Module,
}

impl WasmHost {
    pub fn new(engine: &wasmtime::Engine, module: &wasmtime::Module) -> Result<Self, WasmError> {
        Ok(Self {
            _engine: engine.clone(),
            _module: module.clone(),
        })
    }

    /// Extract manifest from WASM module.
    /// TODO: Instantiate module, call get_manifest export, read JSON from linear memory.
    pub fn get_manifest(&mut self) -> Result<FlowManifest, WasmError> {
        Err(WasmError::Execution(
            "WASM host not yet implemented — use JSON manifest loading for MVP".into(),
        ))
    }

    /// Invoke an action in the WASM module.
    /// TODO: Write ctx as JSON to WASM memory, call invoke_action export, read result.
    pub fn invoke_action(
        &mut self,
        _name: &str,
        _ctx: ActionContext,
    ) -> Result<ActionResult, WasmError> {
        Err(WasmError::Execution(
            "WASM action invocation not yet implemented".into(),
        ))
    }

    /// Evaluate a guard condition.
    /// TODO: Call invoke_guard export.
    pub fn invoke_guard(&mut self, _name: &str, _ctx: GuardContext) -> Result<bool, WasmError> {
        Err(WasmError::Execution(
            "WASM guard invocation not yet implemented".into(),
        ))
    }

    /// Aggregate results from multiple nodes.
    /// TODO: Call aggregate export.
    pub fn aggregate(
        &mut self,
        _name: &str,
        _results: &[NodeResult],
    ) -> Result<AggregateDecision, WasmError> {
        Err(WasmError::Execution(
            "WASM aggregation not yet implemented".into(),
        ))
    }
}
