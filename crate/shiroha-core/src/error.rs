//! 框架级错误类型
//!
//! 统一的错误枚举，覆盖状态机、存储、传输、WASM 各层的错误场景。

use thiserror::Error;

/// 框架统一错误类型
#[derive(Debug, Error)]
pub enum ShirohaError {
    #[error("flow not found: {0}")]
    FlowNotFound(String),

    #[error("job not found: {0}")]
    JobNotFound(String),

    /// 状态机中不存在从 `from` 到 `to` 的合法转移路径
    #[error("invalid transition from `{from}` to `{to}` on event `{event}`")]
    InvalidTransition {
        from: String,
        to: String,
        event: String,
    },

    #[error("guard rejected transition")]
    GuardRejected,

    #[error("action execution failed: {0}")]
    ActionFailed(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("transport error: {0}")]
    Transport(String),

    #[error("wasm error: {0}")]
    Wasm(String),

    /// Job 状态不满足操作前置条件（如暂停一个已暂停的 Job）
    #[error("invalid job state: expected {expected}, got {actual}")]
    InvalidJobState { expected: String, actual: String },
}

pub type Result<T> = std::result::Result<T, ShirohaError>;
