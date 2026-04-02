use thiserror::Error;

#[derive(Debug, Error)]
pub enum ShirohaError {
    #[error("flow not found: {0}")]
    FlowNotFound(String),

    #[error("job not found: {0}")]
    JobNotFound(String),

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

    #[error("invalid job state: expected {expected}, got {actual}")]
    InvalidJobState { expected: String, actual: String },
}

pub type Result<T> = std::result::Result<T, ShirohaError>;
