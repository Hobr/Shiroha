//! Wasmtime 引擎封装
//!
//! [`WasmRuntime`] 管理 wasmtime Engine 实例，提供模块编译入口。
//! Engine 开启了 fuel 消耗，用于限制 WASM 执行步数。

use crate::error::WasmError;

/// WASM 运行时，封装 wasmtime Engine
pub struct WasmRuntime {
    engine: wasmtime::Engine,
}

impl WasmRuntime {
    /// 创建运行时，开启 fuel 消耗限制
    pub fn new() -> Result<Self, WasmError> {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        let engine = wasmtime::Engine::new(&config)?;
        Ok(Self { engine })
    }

    /// 从字节码编译 WASM 模块
    pub fn load_module(&self, wasm_bytes: &[u8]) -> Result<wasmtime::Module, WasmError> {
        wasmtime::Module::new(&self.engine, wasm_bytes)
            .map_err(|e| WasmError::Compile(e.to_string()))
    }

    pub fn engine(&self) -> &wasmtime::Engine {
        &self.engine
    }
}
