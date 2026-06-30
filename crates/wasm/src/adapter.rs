//! WASM adapter implementing the Adapter trait.
//!
//! Note: This is a placeholder implementation for v0.2.0 MVP.
//! The full WASM Component Model integration will be completed after
//! verifying wasmtime 46.x bindgen! macro requirements.

use async_trait::async_trait;
use shiroha_engine::Adapter;
use shiroha_ir::StateMachineDef;

use crate::{Result, WasmError};

/// WASM Component Model adapter for loading state machine definitions.
pub struct WasmAdapter {
    _placeholder: (),
}

impl WasmAdapter {
    /// Create a new WASM adapter from a component file.
    pub fn from_file(_path: impl AsRef<std::path::Path>) -> Result<Self> {
        Err(WasmError::ComponentLoad(
            "WASM Component Model adapter not yet fully implemented in v0.2.0 MVP".to_string(),
        ))
    }

    /// Create a new WASM adapter from component bytes.
    pub fn from_bytes(_bytes: &[u8]) -> Result<Self> {
        Err(WasmError::ComponentLoad(
            "WASM Component Model adapter not yet fully implemented in v0.2.0 MVP".to_string(),
        ))
    }
}

#[async_trait]
impl Adapter for WasmAdapter {
    async fn load(&self) -> anyhow::Result<StateMachineDef> {
        Err(anyhow::anyhow!(
            "WASM Component Model adapter not yet fully implemented in v0.2.0 MVP"
        ))
    }
}
