use thiserror::Error;

use crate::bindings::exports::shiroha::machine::types as wit;
use shiroha_core::{
    ActionKind, ActionOutcome, BusinessFailure, CancelInput, Event, EventName, FunctionDeclaration,
    FunctionId, FunctionRef, FunctionRole, GuardInput, HookEffects, HookInput, HostInput,
    IdentifierError, MachineDefinition, MachineId, PayloadEnvelope, RuntimeFault, RuntimeFaultKind,
    StateDefinition, StateId, TerminalKind, TimeoutInput, TransitionDefinition, Trigger,
};

#[derive(Debug, Error)]
pub enum ConversionError {
    #[error("invalid identifier at `{path}`: {source}")]
    Identifier {
        path: String,
        #[source]
        source: IdentifierError,
    },
    #[error("unsupported action kind `{kind}` at `{path}`")]
    UnsupportedActionKind { path: String, kind: String },
}

pub(crate) fn machine_from_wit(
    definition: wit::MachineDefinition,
) -> Result<MachineDefinition, ConversionError> {
    let machine_id = parse_id::<MachineId>(definition.id, "id")?;
    let initial = parse_id::<StateId>(definition.initial, "initial")?;
    let functions = definition
        .functions
        .into_iter()
        .enumerate()
        .map(|(index, declaration)| {
            Ok(FunctionDeclaration {
                function: function_ref_from_wit(
                    declaration.reference,
                    &format!("functions[{index}].reference"),
                )?,
                role: match declaration.role {
                    wit::FunctionRole::Guard => FunctionRole::Guard,
                    wit::FunctionRole::Action => FunctionRole::Action,
                    wit::FunctionRole::Callback => FunctionRole::Callback,
                },
            })
        })
        .collect::<Result<Vec<_>, ConversionError>>()?;
    let states = definition
        .states
        .into_iter()
        .enumerate()
        .map(|(state_index, state)| state_from_wit(state, state_index))
        .collect::<Result<Vec<_>, ConversionError>>()?;

    Ok(MachineDefinition {
        id: machine_id,
        initial,
        functions,
        states,
    })
}

fn state_from_wit(
    state: wit::StateDefinition,
    state_index: usize,
) -> Result<StateDefinition, ConversionError> {
    let path = format!("states[{state_index}]");
    Ok(StateDefinition {
        id: parse_id(state.id, &format!("{path}.id"))?,
        entry: state
            .entry_callback
            .map(|function| function_ref_from_wit(function, &format!("{path}.entry-callback")))
            .transpose()?,
        exit: state
            .exit_callback
            .map(|function| function_ref_from_wit(function, &format!("{path}.exit-callback")))
            .transpose()?,
        terminal: state.terminal.map(|terminal| match terminal {
            wit::TerminalKind::Completed => TerminalKind::Completed,
            wit::TerminalKind::Failed => TerminalKind::Failed,
            wit::TerminalKind::Cancelled => TerminalKind::Cancelled,
        }),
        transitions: state
            .transitions
            .into_iter()
            .enumerate()
            .map(|(index, transition)| {
                transition_from_wit(transition, &format!("{path}.transitions[{index}]"))
            })
            .collect::<Result<Vec<_>, ConversionError>>()?,
    })
}

fn transition_from_wit(
    transition: wit::TransitionDefinition,
    path: &str,
) -> Result<TransitionDefinition, ConversionError> {
    Ok(TransitionDefinition {
        trigger: match transition.trigger {
            wit::Trigger::Event(name) => {
                Trigger::Event(parse_id(name, &format!("{path}.trigger.event"))?)
            }
            wit::Trigger::Timeout(key) => {
                Trigger::Timeout(parse_id(key, &format!("{path}.trigger.timeout"))?)
            }
            wit::Trigger::Cancel => Trigger::Cancel,
        },
        guard: transition
            .guard
            .map(|function| function_ref_from_wit(function, &format!("{path}.guard")))
            .transpose()?,
        action: transition
            .action
            .map(|function| function_ref_from_wit(function, &format!("{path}.action")))
            .transpose()?,
        target: parse_id(transition.target, &format!("{path}.target"))?,
        failure_target: transition
            .failure_target
            .map(|target| parse_id(target, &format!("{path}.failure-target")))
            .transpose()?,
    })
}

fn function_ref_from_wit(
    function: wit::FunctionRef,
    path: &str,
) -> Result<FunctionRef, ConversionError> {
    let kind = parse_id::<ActionKind>(function.kind, &format!("{path}.kind"))?;
    if kind != ActionKind::wasm_component() {
        return Err(ConversionError::UnsupportedActionKind {
            path: path.to_owned(),
            kind: kind.into_string(),
        });
    }
    Ok(FunctionRef {
        kind,
        locator: parse_id::<FunctionId>(function.locator, &format!("{path}.locator"))?,
    })
}

fn parse_id<T>(value: String, path: &str) -> Result<T, ConversionError>
where
    T: TryFrom<String, Error = IdentifierError>,
{
    T::try_from(value).map_err(|source| ConversionError::Identifier {
        path: path.to_owned(),
        source,
    })
}

pub(crate) fn guard_input_to_wit(input: GuardInput) -> wit::GuardInput {
    wit::GuardInput {
        source_state: input.source_state.into_string(),
        context: payload_to_wit(input.context),
        input: host_input_to_wit(input.input),
    }
}

pub(crate) fn hook_input_to_wit(input: HookInput) -> wit::HookInput {
    wit::HookInput {
        source_state: input.source_state.into_string(),
        target_state: input.target_state.map(StateId::into_string),
        context: payload_to_wit(input.context),
        input: host_input_to_wit(input.input),
    }
}

fn host_input_to_wit(input: HostInput) -> wit::HostInput {
    match input {
        HostInput::Start => wit::HostInput::Start,
        HostInput::Event(event) => wit::HostInput::Event(event_to_wit(event)),
        HostInput::Timeout(TimeoutInput { key, payload }) => {
            wit::HostInput::Timeout(wit::TimeoutInput {
                key: key.into_string(),
                payload: payload.map(payload_to_wit),
            })
        }
        HostInput::Cancel(CancelInput { reason }) => wit::HostInput::Cancel(wit::CancelInput {
            reason: reason.map(payload_to_wit),
        }),
    }
}

fn event_to_wit(event: Event) -> wit::Event {
    wit::Event {
        name: event.name.into_string(),
        payload: event.payload.map(payload_to_wit),
    }
}

fn payload_to_wit(payload: PayloadEnvelope) -> wit::Payload {
    wit::Payload {
        data: payload.data().to_vec(),
        content_type: payload.content_type().to_owned(),
        schema_id: payload.schema_id().map(str::to_owned),
    }
}

fn payload_from_wit(payload: wit::Payload) -> PayloadEnvelope {
    PayloadEnvelope::new(payload.data, payload.content_type, payload.schema_id)
}

pub(crate) fn effects_from_wit(effects: wit::HookEffects) -> Result<HookEffects, ConversionError> {
    Ok(HookEffects {
        replacement_context: effects.replacement_context.map(payload_from_wit),
        events: effects
            .events
            .into_iter()
            .enumerate()
            .map(|(index, event)| {
                Ok(Event {
                    name: parse_id::<EventName>(
                        event.name,
                        &format!("effects.events[{index}].name"),
                    )?,
                    payload: event.payload.map(payload_from_wit),
                })
            })
            .collect::<Result<Vec<_>, ConversionError>>()?,
    })
}

pub(crate) fn action_from_wit(
    outcome: wit::ActionOutcome,
) -> Result<ActionOutcome, ConversionError> {
    match outcome {
        wit::ActionOutcome::Succeeded(effects) => {
            Ok(ActionOutcome::Succeeded(effects_from_wit(effects)?))
        }
        wit::ActionOutcome::Failed(failure) => Ok(ActionOutcome::Failed(BusinessFailure {
            code: failure.code,
            payload: failure.payload.map(payload_from_wit),
            effects: effects_from_wit(failure.effects)?,
        })),
    }
}

pub(crate) fn guest_error_to_fault(error: wit::GuestError) -> RuntimeFault {
    RuntimeFault::new(RuntimeFaultKind::Guest, error.message)
        .with_guest_details(error.code, error.payload.map(payload_from_wit))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_envelope_round_trips_without_interpretation() {
        let original = PayloadEnvelope::new(
            vec![0, 1, 2, 255],
            "application/json",
            Some("schema:test".to_owned()),
        );
        let round_trip = payload_from_wit(payload_to_wit(original.clone()));
        assert_eq!(round_trip, original);
    }

    #[test]
    fn conversion_rejects_non_component_action_kinds() {
        let error = function_ref_from_wit(
            wit::FunctionRef {
                kind: "http".to_owned(),
                locator: "request".to_owned(),
            },
            "functions[0]",
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ConversionError::UnsupportedActionKind { .. }
        ));
    }
}
