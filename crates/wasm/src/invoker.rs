//! WASM action invoker implementing ActionInvoker trait.
//!
//! Note: This is a placeholder implementation for v0.2.0 MVP.
//! The full WASM Component Model integration will be completed after
//! verifying wasmtime 46.x bindgen! macro requirements.

use async_trait::async_trait;
use shiroha_engine::{ActionContext, ActionInvoker, ActionResult};

use crate::{Result, WasmError};

/// WASM action invoker for executing actions defined in WASM components.
pub struct WasmActionInvoker {
    _placeholder: (),
}

impl WasmActionInvoker {
    /// Create a new WASM action invoker from a component file.
    pub fn from_file(_path: impl AsRef<std::path::Path>) -> Result<Self> {
        Err(WasmError::ComponentLoad(
            "WASM action invoker not yet fully implemented in v0.2.0 MVP".to_string(),
        ))
    }

    /// Create a new WASM action invoker from component bytes.
    pub fn from_bytes(_bytes: &[u8]) -> Result<Self> {
        Err(WasmError::ComponentLoad(
            "WASM action invoker not yet fully implemented in v0.2.0 MVP".to_string(),
        ))
    }
}

#[async_trait]
impl ActionInvoker for WasmActionInvoker {
    async fn invoke_sync(&self, _name: &str, _ctx: ActionContext) -> anyhow::Result<ActionResult> {
        Err(anyhow::anyhow!(
            "WASM action invoker not yet fully implemented in v0.2.0 MVP"
        ))
    }

    async fn invoke_do(&self, _name: &str, _ctx: ActionContext) -> anyhow::Result<ActionResult> {
        Err(anyhow::anyhow!(
            "WASM action invoker not yet fully implemented in v0.2.0 MVP"
        ))
    }
}
