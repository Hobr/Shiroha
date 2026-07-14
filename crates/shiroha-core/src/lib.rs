//! Runtime-neutral finite-state-machine engine for Shiroha.

#![forbid(unsafe_code)]

mod engine;
mod executor;
mod id;
mod limits;
mod model;
mod runtime;
mod validation;

pub use engine::MachineInstance;
pub use executor::{
    AdapterError, ArtifactBytes, DefinitionAdapter, FunctionExecutor, FunctionExecutorFactory,
};
pub use id::{
    ActionKind, EventName, FunctionId, IdentifierError, InstanceId, MachineId, StateId, TimeoutKey,
};
pub use limits::{CpuBudget, InvocationLimits, LimitsError, LoadLimits, RuntimeLimits};
pub use model::{
    ActionOutcome, BusinessFailure, CancelInput, Event, FunctionDeclaration, FunctionRef,
    FunctionRole, GuardInput, HookEffects, HookInput, HostInput, MachineDefinition,
    PayloadEnvelope, StateDefinition, TerminalKind, TimeoutInput, TransitionDefinition, Trigger,
};
pub use runtime::{
    BusinessFailureRecord, DispatchError, FailureRecord, Lifecycle, MachineSnapshot,
    ResourceLimitKind, RunOutcome, RunReport, RuntimeFault, RuntimeFaultKind, StartError,
    StepOutcome, StepRecord, UnhandledInput,
};
pub use validation::{
    PreparationWarning, ValidatedMachine, ValidationCode, ValidationErrors, ValidationIssue,
};
