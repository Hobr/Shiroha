//! Wasmtime 引擎封装
//!
//! [`WasmRuntime`] 管理 wasmtime Engine 实例，提供 component 编译入口。
//! Engine 开启了 fuel 消耗，并启用 component model。

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
        config.wasm_component_model(true);
        let engine = wasmtime::Engine::new(&config)?;
        Ok(Self { engine })
    }

    /// 从字节码编译 component。
    pub fn load_component(
        &self,
        wasm_bytes: &[u8],
    ) -> Result<wasmtime::component::Component, WasmError> {
        wasmtime::component::Component::new(&self.engine, wasm_bytes)
            .map_err(|e| WasmError::Compile(e.to_string()))
    }

    pub fn engine(&self) -> &wasmtime::Engine {
        &self.engine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_component_accepts_valid_components() {
        let runtime = WasmRuntime::new().expect("runtime");
        runtime
            .load_component(b"(component)")
            .expect("component should compile");
    }

    #[test]
    fn load_component_rejects_core_modules() {
        let runtime = WasmRuntime::new().expect("runtime");
        let error = match runtime.load_component(b"(module (func (export \"f\")))") {
            Ok(_) => panic!("core module should be rejected"),
            Err(error) => error,
        };

        assert!(matches!(error, WasmError::Compile(_)));
    }
}
