//! WASM component 缓存
//!
//! [`WasmModule`] 封装编译后的 wasmtime component 并关联内容哈希。
//! [`ModuleCache`] 按哈希缓存已编译 component，避免重复编译。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};

/// 编译后的 WASM component，附带内容哈希用于缓存索引
pub struct WasmModule {
    component: wasmtime::component::Component,
    hash: String,
}

impl WasmModule {
    pub fn new(component: wasmtime::component::Component, wasm_bytes: &[u8]) -> Self {
        let hash = Self::compute_hash(wasm_bytes);
        Self { component, hash }
    }

    pub fn hash(&self) -> &str {
        &self.hash
    }

    pub fn component(&self) -> &wasmtime::component::Component {
        &self.component
    }

    fn compute_hash(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 模块缓存，按内容哈希索引
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
        // 只在短临界区内 clone `Arc`，避免长时间持有全局缓存锁。
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

#[cfg(test)]
mod tests {
    use super::WasmModule;

    #[test]
    fn compute_hash_changes_when_middle_bytes_change() {
        let left = vec![0_u8; 64];
        let mut right = left.clone();
        right[32] = 1;

        assert_ne!(
            WasmModule::compute_hash(&left),
            WasmModule::compute_hash(&right),
            "hash should include the full wasm payload"
        );
    }
}
