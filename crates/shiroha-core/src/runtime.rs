use thiserror::Error;

use crate::{HostInput, InstanceId, MachineId, PayloadEnvelope, StateId, TerminalKind};

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ResourceLimitKind {
    #[error("CPU budget")]
    Cpu,
    #[error("wall-time deadline")]
    Deadline,
    #[error("linear memory")]
    Memory,
    #[error("payload size")]
    Payload,
    #[error("events emitted by one hook")]
    Events,
    #[error("run-to-completion microsteps")]
    Microsteps,
    #[error("runtime resources")]
    RuntimeResources,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RuntimeFaultKind {
    #[error("guest-declared error")]
    Guest,
    #[error("guest trap or canonical ABI fault")]
    Trap,
    #[error("resource limit exceeded: {0}")]
    ResourceLimit(ResourceLimitKind),
    #[error("runtime engine error")]
    Engine,
    #[error("Host error")]
    Host,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("{kind}: {message}")]
pub struct RuntimeFault {
    pub kind: RuntimeFaultKind,
    pub code: Option<String>,
    pub message: String,
    pub payload: Option<PayloadEnvelope>,
    pub external_effects_possible: bool,
}

impl RuntimeFault {
    #[must_use]
    pub fn new(kind: RuntimeFaultKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            code: None,
            message: message.into(),
            payload: None,
            external_effects_possible: false,
        }
    }

    #[must_use]
    pub fn with_guest_details(
        mut self,
        code: impl Into<String>,
        payload: Option<PayloadEnvelope>,
    ) -> Self {
        self.code = Some(code.into());
        self.payload = payload;
        self
    }

    #[must_use]
    pub fn with_external_effects_possible(mut self, possible: bool) -> Self {
        self.external_effects_possible = possible;
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BusinessFailureRecord {
    pub code: String,
    pub payload: Option<PayloadEnvelope>,
    pub external_effects_possible: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FailureRecord {
    Business(BusinessFailureRecord),
    Runtime(RuntimeFault),
    Terminal { state: StateId },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Lifecycle {
    Active,
    Completed,
    Failed(FailureRecord),
    Cancelled { reason: Option<PayloadEnvelope> },
}

impl Lifecycle {
    #[must_use]
    pub fn from_terminal(terminal: Option<TerminalKind>, state: &StateId) -> Self {
        match terminal {
            None => Self::Active,
            Some(TerminalKind::Completed) => Self::Completed,
            Some(TerminalKind::Failed) => Self::Failed(FailureRecord::Terminal {
                state: state.clone(),
            }),
            Some(TerminalKind::Cancelled) => Self::Cancelled { reason: None },
        }
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineSnapshot {
    pub machine_id: MachineId,
    pub instance_id: InstanceId,
    pub sequence: u64,
    pub state: StateId,
    pub context: PayloadEnvelope,
    pub lifecycle: Lifecycle,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StepOutcome {
    Transitioned {
        transition_index: usize,
        business_failure: Option<BusinessFailureRecord>,
    },
    BusinessFailed(BusinessFailureRecord),
    Unhandled,
    Cancelled,
    Fault(RuntimeFault),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepRecord {
    pub input: HostInput,
    pub source: StateId,
    pub target: Option<StateId>,
    pub sequence: u64,
    pub outcome: StepOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnhandledInput {
    pub state: StateId,
    pub input: HostInput,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RunOutcome {
    Quiescent,
    Completed,
    Failed(FailureRecord),
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RunReport {
    pub start_sequence: u64,
    pub end_sequence: u64,
    pub microsteps: usize,
    pub steps: Vec<StepRecord>,
    pub unhandled: Vec<UnhandledInput>,
    pub outcome: RunOutcome,
}

#[derive(Clone, Debug, Error, PartialEq)]
#[error("machine startup failed before the initial snapshot committed: {fault}")]
pub struct StartError {
    pub attempted_state: StateId,
    pub attempted_context: PayloadEnvelope,
    pub fault: RuntimeFault,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum DispatchError {
    #[error("machine is not active: {0:?}")]
    NotActive(Lifecycle),
    #[error("invalid runtime limits: {0}")]
    InvalidLimits(#[from] crate::LimitsError),
    #[error("input payload exceeds the configured limit")]
    PayloadTooLarge,
    #[error("snapshot belongs to machine `{actual}`, expected `{expected}`")]
    SnapshotMachineMismatch {
        expected: MachineId,
        actual: MachineId,
    },
    #[error("startup is an internal lifecycle input and cannot be dispatched")]
    StartupInput,
}
