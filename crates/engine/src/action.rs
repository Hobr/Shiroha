//! Action execution and invoker traits.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Context passed to actions during execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionContext {
    pub task_id: String,
    pub event: Option<String>,
    pub payload: Option<Vec<u8>>,
}

/// Result of action execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionResult {
    /// Action completed successfully with no value.
    Ok,
    /// Action completed successfully with a value.
    OkValue(Vec<u8>),
    /// Action failed with an error message.
    Error(String),
    /// Action produced a signal that should be injected as an internal event.
    Signal(String),
}

/// Trait for invoking actions (both sync and async do-activities).
#[async_trait]
pub trait ActionInvoker: Send + Sync {
    /// Invoke a synchronous action (entry/exit/transition).
    /// Must be fast and non-blocking.
    async fn invoke_sync(&self, name: &str, ctx: ActionContext) -> anyhow::Result<ActionResult>;

    /// Invoke an async do-activity.
    /// Can be long-running and is cancellable via task cancellation.
    async fn invoke_do(&self, name: &str, ctx: ActionContext) -> anyhow::Result<ActionResult>;
}

/// Trait for evaluating guards.
#[async_trait]
pub trait GuardEvaluator: Send + Sync {
    /// Evaluate a guard condition.
    async fn evaluate(&self, guard: &str, ctx: &ActionContext) -> anyhow::Result<bool>;
}
