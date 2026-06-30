//! # shiroha-wasm
//!
//! WASM Component Model adapter for state machine definitions.
//! This crate provides the ability to load state machine definitions from
//! WebAssembly components and execute actions within the WASM runtime.

mod adapter;
mod error;
mod host;
mod invoker;

pub use adapter::WasmAdapter;
pub use error::{Result, WasmError};
pub use invoker::WasmActionInvoker;

// Re-export wasmtime types that users might need
pub use wasmtime::{Config, Engine, Store};
