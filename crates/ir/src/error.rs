//! Error types for IR validation and construction.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IrError {
    #[error("Initial state '{0}' does not exist")]
    InitialStateNotFound(String),

    #[error("Parent state '{0}' does not exist")]
    ParentStateNotFound(String),

    #[error("Circular nesting detected: {0}")]
    CircularNesting(String),

    #[error("Event '{0}' not found")]
    EventNotFound(String),

    #[error("Transition from '{from}' to '{to}' references unknown event '{event}'")]
    TransitionEventNotFound {
        from: String,
        to: String,
        event: String,
    },

    #[error("Transition from '{from}' to '{to}' references unknown state")]
    TransitionStateNotFound { from: String, to: String },

    #[error("State '{0}' not found")]
    StateNotFound(String),

    #[error("Invalid history configuration: {0}")]
    InvalidHistory(String),

    #[error("Duplicate state id: {0}")]
    DuplicateStateId(String),

    #[error("Duplicate event id: {0}")]
    DuplicateEventId(String),
}

pub type Result<T> = std::result::Result<T, IrError>;
