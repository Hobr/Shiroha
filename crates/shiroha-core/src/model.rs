use std::sync::Arc;

use crate::{ActionKind, EventName, FunctionId, StateId, TimeoutKey};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayloadEnvelope {
    data: Arc<[u8]>,
    content_type: String,
    schema_id: Option<String>,
}

impl PayloadEnvelope {
    #[must_use]
    pub fn new(
        data: impl Into<Vec<u8>>,
        content_type: impl Into<String>,
        schema_id: Option<String>,
    ) -> Self {
        Self {
            data: Arc::from(data.into()),
            content_type: content_type.into(),
            schema_id,
        }
    }

    #[must_use]
    pub fn json(data: impl Into<Vec<u8>>) -> Self {
        Self::new(data, "application/json", None)
    }

    #[must_use]
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    #[must_use]
    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    #[must_use]
    pub fn schema_id(&self) -> Option<&str> {
        self.schema_id.as_deref()
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Trigger {
    Event(EventName),
    Timeout(TimeoutKey),
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalKind {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FunctionRole {
    Guard,
    Action,
    Callback,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FunctionRef {
    pub kind: ActionKind,
    pub locator: FunctionId,
}

impl FunctionRef {
    #[must_use]
    pub fn wasm(locator: FunctionId) -> Self {
        Self {
            kind: ActionKind::wasm_component(),
            locator,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionDeclaration {
    pub function: FunctionRef,
    pub role: FunctionRole,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionDefinition {
    pub trigger: Trigger,
    pub guard: Option<FunctionRef>,
    pub action: Option<FunctionRef>,
    pub target: StateId,
    pub failure_target: Option<StateId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateDefinition {
    pub id: StateId,
    pub entry: Option<FunctionRef>,
    pub exit: Option<FunctionRef>,
    pub terminal: Option<TerminalKind>,
    pub transitions: Vec<TransitionDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineDefinition {
    pub id: crate::MachineId,
    pub initial: StateId,
    pub functions: Vec<FunctionDeclaration>,
    pub states: Vec<StateDefinition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Event {
    pub name: EventName,
    pub payload: Option<PayloadEnvelope>,
}

impl Event {
    #[must_use]
    pub fn new(name: EventName, payload: Option<PayloadEnvelope>) -> Self {
        Self { name, payload }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimeoutInput {
    pub key: TimeoutKey,
    pub payload: Option<PayloadEnvelope>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CancelInput {
    pub reason: Option<PayloadEnvelope>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostInput {
    Start,
    Event(Event),
    Timeout(TimeoutInput),
    Cancel(CancelInput),
}

impl HostInput {
    #[must_use]
    pub fn trigger(&self) -> Option<Trigger> {
        match self {
            Self::Start => None,
            Self::Event(event) => Some(Trigger::Event(event.name.clone())),
            Self::Timeout(timeout) => Some(Trigger::Timeout(timeout.key.clone())),
            Self::Cancel(_) => Some(Trigger::Cancel),
        }
    }

    #[must_use]
    pub fn payload(&self) -> Option<&PayloadEnvelope> {
        match self {
            Self::Start => None,
            Self::Event(event) => event.payload.as_ref(),
            Self::Timeout(timeout) => timeout.payload.as_ref(),
            Self::Cancel(cancel) => cancel.reason.as_ref(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardInput {
    pub source_state: StateId,
    pub context: PayloadEnvelope,
    pub input: HostInput,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HookInput {
    pub source_state: StateId,
    pub target_state: Option<StateId>,
    pub context: PayloadEnvelope,
    pub input: HostInput,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HookEffects {
    pub replacement_context: Option<PayloadEnvelope>,
    pub events: Vec<Event>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BusinessFailure {
    pub code: String,
    pub payload: Option<PayloadEnvelope>,
    pub effects: HookEffects,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ActionOutcome {
    Succeeded(HookEffects),
    Failed(BusinessFailure),
}
