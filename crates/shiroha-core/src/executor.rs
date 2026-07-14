use std::sync::Arc;

use async_trait::async_trait;
use thiserror::Error;

use crate::{
    ActionOutcome, FunctionRef, GuardInput, HookEffects, HookInput, InvocationLimits, LoadLimits,
    MachineDefinition, RuntimeFault,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactBytes(Arc<[u8]>);

impl ArtifactBytes {
    #[must_use]
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(Arc::from(bytes.into()))
    }

    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("definition adapter failed: {message}")]
pub struct AdapterError {
    pub message: String,
}

#[async_trait]
pub trait DefinitionAdapter: Send + Sync {
    async fn load_definition(
        &self,
        artifact: ArtifactBytes,
        limits: &LoadLimits,
    ) -> Result<MachineDefinition, AdapterError>;
}

#[async_trait]
pub trait FunctionExecutor: Send {
    async fn evaluate_guard(
        &mut self,
        function: &FunctionRef,
        input: GuardInput,
        limits: &InvocationLimits,
    ) -> Result<bool, RuntimeFault>;

    async fn invoke_callback(
        &mut self,
        function: &FunctionRef,
        input: HookInput,
        limits: &InvocationLimits,
    ) -> Result<HookEffects, RuntimeFault>;

    async fn invoke_action(
        &mut self,
        function: &FunctionRef,
        input: HookInput,
        limits: &InvocationLimits,
    ) -> Result<ActionOutcome, RuntimeFault>;
}

#[async_trait]
pub trait FunctionExecutorFactory: Send + Sync {
    async fn create(&self) -> Result<Box<dyn FunctionExecutor>, RuntimeFault>;
}
