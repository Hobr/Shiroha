//! Wasmtime Component Model adapter for Shiroha.

#![deny(unsafe_code)]

pub mod convert;
pub mod runtime;

mod bindings;
mod error;
mod executor;
mod loader;

pub use convert::ConversionError;
pub use error::WasmError;
pub use executor::WasmExecutorFactory;
pub use loader::{PreparationMetadata, PreparedWasmMachine, WasmMachineLoader};
/// The adapter implementation is introduced after the runtime-neutral Core
/// contract is covered by tests.
pub const ADAPTER_NAME: &str = "wasm-component";
