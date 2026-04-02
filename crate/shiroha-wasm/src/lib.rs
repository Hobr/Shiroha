//! Shiroha WASM 运行时
//!
//! 基于 wasmtime 的 WASM 模块加载、缓存和执行层。
//! 提供 host-guest 桥接接口，用于调用 Flow WASM 模块导出的函数。
//!
//! Phase 1 (MVP)：API 接口已定义，实际 host-guest 协议待实现。
// WASM 层错误类型
pub mod error;
// WASM Host-Guest 桥接层
pub mod host;
// WASM 模块缓存
pub mod module_cache;
// Wasmtime 引擎封装
pub mod runtime;
