use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct WasmModule {
    module: wasmtime::Module,
    hash: String,
}

impl WasmModule {
    pub fn new(module: wasmtime::Module, wasm_bytes: &[u8]) -> Self {
        let hash = Self::compute_hash(wasm_bytes);
        Self { module, hash }
    }

    pub fn hash(&self) -> &str {
        &self.hash
    }

    pub fn module(&self) -> &wasmtime::Module {
        &self.module
    }

    fn compute_hash(bytes: &[u8]) -> String {
        // Simple hash for MVP: length + first/last bytes
        let len = bytes.len();
        let head: Vec<u8> = bytes.iter().take(16).copied().collect();
        let tail: Vec<u8> = bytes.iter().rev().take(16).copied().collect();
        format!("{len:016x}-{}-{}", hex(&head), hex(&tail))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub struct ModuleCache {
    modules: Mutex<HashMap<String, Arc<WasmModule>>>,
}

impl ModuleCache {
    pub fn new() -> Self {
        Self {
            modules: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, hash: &str) -> Option<Arc<WasmModule>> {
        self.modules.lock().unwrap().get(hash).cloned()
    }

    pub fn insert(&self, module: Arc<WasmModule>) {
        self.modules
            .lock()
            .unwrap()
            .insert(module.hash().to_string(), module);
    }
}

impl Default for ModuleCache {
    fn default() -> Self {
        Self::new()
    }
}
