//! WASM 层错误类型

use shiroha_core::error::ShirohaError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WasmError {
    #[error("wasm compilation error: {0}")]
    Compile(String),
    #[error("wasm instantiation error: {0}")]
    Instantiation(String),
    #[error("wasm execution error: {0}")]
    Execution(String),
    #[error("wasm memory error: {0}")]
    Memory(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

impl From<wasmtime::Error> for WasmError {
    fn from(e: wasmtime::Error) -> Self {
        // 大多数 wasmtime 运行期错误最终都在调用阶段暴露，统一归到 Execution。
        Self::Execution(e.to_string())
    }
}

impl From<WasmError> for ShirohaError {
    fn from(e: WasmError) -> Self {
        // 向 core 层收敛后不再暴露 wasmtime 细节，只保留文本描述。
        ShirohaError::Wasm(e.to_string())
    }
}
