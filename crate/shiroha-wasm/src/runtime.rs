use crate::error::WasmError;

pub struct WasmRuntime {
    engine: wasmtime::Engine,
}

impl WasmRuntime {
    pub fn new() -> Result<Self, WasmError> {
        let mut config = wasmtime::Config::new();
        config.consume_fuel(true);
        let engine = wasmtime::Engine::new(&config)?;
        Ok(Self { engine })
    }

    pub fn load_module(&self, wasm_bytes: &[u8]) -> Result<wasmtime::Module, WasmError> {
        wasmtime::Module::new(&self.engine, wasm_bytes)
            .map_err(|e| WasmError::Compile(e.to_string()))
    }

    pub fn engine(&self) -> &wasmtime::Engine {
        &self.engine
    }
}
