//! Error types for WASM adapter.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, WasmError>;

#[derive(Error, Debug)]
pub enum WasmError {
    #[error("Failed to load WASM component: {0}")]
    ComponentLoad(String),

    #[error("Failed to instantiate WASM component: {0}")]
    Instantiation(String),

    #[error("Failed to call WASM function: {0}")]
    FunctionCall(String),

    #[error("Failed to convert WIT types: {0}")]
    TypeConversion(String),

    #[error("WASM runtime error: {0}")]
    Runtime(#[from] anyhow::Error),

    #[error("Wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),

    #[error("IR error: {0}")]
    Ir(#[from] shiroha_ir::IrError),
}
