//! WASM component 缓存
//!
//! [`WasmModule`] 封装编译后的 wasmtime component 并关联内容哈希。
//! [`ModuleCache`] 按哈希缓存已编译 component，避免重复编译。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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

    /// 简易哈希（MVP）：长度 + 首尾各16字节的十六进制
    ///
    /// 生产环境应替换为 SHA-256。
    fn compute_hash(bytes: &[u8]) -> String {
        let len = bytes.len();
        let head: Vec<u8> = bytes.iter().take(16).copied().collect();
        let tail: Vec<u8> = bytes.iter().rev().take(16).copied().collect();
        format!("{len:016x}-{}-{}", hex(&head), hex(&tail))
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
