//! Public Shiroha runtime facade.

#![forbid(unsafe_code)]

use shiroha_adapter_wasm::{
    PreparationMetadata, PreparedWasmMachine, WasmError, WasmMachineLoader,
};
use shiroha_core::{
    ArtifactBytes, DispatchError, HostInput, LoadLimits, MachineSnapshot, PayloadEnvelope,
    RunReport, RuntimeFault, RuntimeLimits, StartError,
};
use thiserror::Error;

pub use shiroha_core as core;
pub use shiroha_core::{
    CancelInput, Event, EventName, Lifecycle, RunOutcome, StateId, TimeoutInput, TimeoutKey,
};

#[derive(Clone)]
pub struct ShirohaRuntime {
    loader: WasmMachineLoader,
    load_limits: LoadLimits,
    runtime_limits: RuntimeLimits,
}

impl ShirohaRuntime {
    #[must_use]
    pub fn builder() -> ShirohaRuntimeBuilder {
        ShirohaRuntimeBuilder::default()
    }

    pub async fn prepare_component(
        &self,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<PreparedMachine, PrepareError> {
        let prepared = self
            .loader
            .prepare(ArtifactBytes::new(bytes), &self.load_limits)
            .await?;
        Ok(PreparedMachine {
            prepared,
            runtime_limits: self.runtime_limits.clone(),
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ShirohaRuntimeBuilder {
    load_limits: LoadLimits,
    runtime_limits: RuntimeLimits,
}

impl ShirohaRuntimeBuilder {
    #[must_use]
    pub fn load_limits(mut self, limits: LoadLimits) -> Self {
        self.load_limits = limits;
        self
    }

    #[must_use]
    pub fn runtime_limits(mut self, limits: RuntimeLimits) -> Self {
        self.runtime_limits = limits;
        self
    }

    pub fn build(self) -> Result<ShirohaRuntime, BuildError> {
        self.load_limits.validate()?;
        self.runtime_limits.validate()?;
        let loader = WasmMachineLoader::new(self.runtime_limits.invocation.clone())?;
        Ok(ShirohaRuntime {
            loader,
            load_limits: self.load_limits,
            runtime_limits: self.runtime_limits,
        })
    }
}

#[derive(Clone)]
pub struct PreparedMachine {
    prepared: PreparedWasmMachine,
    runtime_limits: RuntimeLimits,
}

impl PreparedMachine {
    #[must_use]
    pub fn metadata(&self) -> &PreparationMetadata {
        self.prepared.metadata()
    }

    pub async fn start(
        &self,
        initial_context: PayloadEnvelope,
    ) -> Result<LocalMachine, StartMachineError> {
        let executor = self.prepared.create_executor().await?;
        let machine = shiroha_core::MachineInstance::start(
            self.prepared.definition().clone(),
            executor,
            initial_context,
            self.runtime_limits.clone(),
        )
        .await?;
        Ok(LocalMachine { machine })
    }

    pub async fn restore(
        &self,
        snapshot: MachineSnapshot,
    ) -> Result<LocalMachine, RestoreMachineError> {
        let executor = self.prepared.create_executor().await?;
        let machine = shiroha_core::MachineInstance::restore(
            self.prepared.definition().clone(),
            snapshot,
            executor,
            self.runtime_limits.clone(),
        )?;
        Ok(LocalMachine { machine })
    }
}

pub struct LocalMachine {
    machine: shiroha_core::MachineInstance,
}

impl LocalMachine {
    #[must_use]
    pub fn snapshot(&self) -> &MachineSnapshot {
        self.machine.snapshot()
    }

    pub async fn dispatch(&mut self, input: HostInput) -> Result<RunReport, DispatchError> {
        self.machine.dispatch(input).await
    }
}

#[derive(Debug, Error)]
pub enum BuildError {
    #[error(transparent)]
    Limits(#[from] shiroha_core::LimitsError),
    #[error(transparent)]
    Wasm(#[from] WasmError),
}

#[derive(Debug, Error)]
#[error(transparent)]
pub struct PrepareError(#[from] WasmError);

#[derive(Debug, Error)]
pub enum StartMachineError {
    #[error("failed to create guest executor: {0}")]
    Executor(#[from] RuntimeFault),
    #[error(transparent)]
    Start(#[from] StartError),
}

#[derive(Debug, Error)]
pub enum RestoreMachineError {
    #[error("failed to create guest executor: {0}")]
    Executor(#[from] RuntimeFault),
    #[error(transparent)]
    Snapshot(#[from] DispatchError),
}
