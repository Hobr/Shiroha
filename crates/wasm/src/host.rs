//! Host interface implementation for WASM components.
//!
//! Implements the host capabilities that WASM components can import.

use crate::bindings::shiroha::sm::host::{Host, LogLevel};

/// Host implementation providing logging capability to WASM components.
pub struct HostImpl;

impl Host for HostImpl {
    fn log(&mut self, level: LogLevel, msg: String) {
        match level {
            LogLevel::Trace => tracing::trace!("[WASM] {}", msg),
            LogLevel::Debug => tracing::debug!("[WASM] {}", msg),
            LogLevel::Info => tracing::info!("[WASM] {}", msg),
            LogLevel::Warn => tracing::warn!("[WASM] {}", msg),
            LogLevel::Error => tracing::error!("[WASM] {}", msg),
        }
    }
}

// Implement empty Host traits for other interfaces that don't have functions
impl crate::bindings::shiroha::sm::types::Host for HostImpl {}
impl crate::bindings::shiroha::sm::action_types::Host for HostImpl {}
