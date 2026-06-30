//! Adapter traits for loading state machine definitions.

use async_trait::async_trait;
use shiroha_ir::StateMachineDef;

/// Trait for adapters that load state machine definitions.
#[async_trait]
pub trait Adapter: Send + Sync {
    /// Load a state machine definition from the source.
    async fn load(&self) -> anyhow::Result<StateMachineDef>;
}

/// Trait for authorizing task creation (capability hook).
#[async_trait]
pub trait Authorizer: Send + Sync {
    /// Authorize a task creation request.
    /// Default implementation allows all requests (no-op).
    async fn authorize(&self, _req: AuthorizeReq) -> Result<(), AuthzError> {
        Ok(())
    }
}

/// Authorization request for task creation.
#[derive(Debug, Clone)]
pub struct AuthorizeReq {
    pub definition_id: String,
    pub capabilities: Vec<String>,
}

/// Authorization error.
#[derive(Debug, thiserror::Error)]
pub enum AuthzError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}
