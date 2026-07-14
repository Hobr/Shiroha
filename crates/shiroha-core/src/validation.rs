use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::sync::Arc;

use thiserror::Error;
use tracing::{instrument, warn};

use crate::{
    FunctionRef, FunctionRole, LoadLimits, MachineDefinition, StateDefinition, StateId,
    TransitionDefinition, Trigger,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ValidationCode {
    LimitExceeded,
    DuplicateState,
    DuplicateFunction,
    MissingInitialState,
    MissingTarget,
    MissingFunction,
    FunctionRoleMismatch,
    FailureTargetWithoutAction,
    TerminalStateHasTransitions,
}

impl fmt::Display for ValidationCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::LimitExceeded => "limit-exceeded",
            Self::DuplicateState => "duplicate-state",
            Self::DuplicateFunction => "duplicate-function",
            Self::MissingInitialState => "missing-initial-state",
            Self::MissingTarget => "missing-target",
            Self::MissingFunction => "missing-function",
            Self::FunctionRoleMismatch => "function-role-mismatch",
            Self::FailureTargetWithoutAction => "failure-target-without-action",
            Self::TerminalStateHasTransitions => "terminal-state-has-transitions",
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationIssue {
    pub code: ValidationCode,
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
#[error("machine definition has {issue_count} validation issue(s)", issue_count = .issues.len())]
pub struct ValidationErrors {
    pub issues: Vec<ValidationIssue>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PreparationWarning {
    UnreachableState { state: StateId },
}

#[derive(Clone, Debug)]
pub struct ValidatedMachine {
    definition: Arc<MachineDefinition>,
    state_index: HashMap<StateId, usize>,
    trigger_index: Vec<HashMap<Trigger, Vec<usize>>>,
    function_index: HashMap<FunctionRef, FunctionRole>,
    warnings: Vec<PreparationWarning>,
}

impl ValidatedMachine {
    #[instrument(
        name = "shiroha.validate",
        skip_all,
        fields(machine = %definition.id, states = definition.states.len())
    )]
    pub fn new(
        definition: MachineDefinition,
        limits: &LoadLimits,
    ) -> Result<Self, ValidationErrors> {
        let mut issues = Vec::new();

        if let Err(error) = limits.validate() {
            issues.push(issue(
                ValidationCode::LimitExceeded,
                "limits",
                error.to_string(),
            ));
        }
        check_count(
            definition.states.len(),
            limits.max_states,
            "states",
            &mut issues,
        );
        check_count(
            definition.functions.len(),
            limits.max_functions,
            "functions",
            &mut issues,
        );
        let transition_count = definition
            .states
            .iter()
            .map(|state| state.transitions.len())
            .sum();
        check_count(
            transition_count,
            limits.max_transitions,
            "transitions",
            &mut issues,
        );

        let mut function_index = HashMap::new();
        for (index, declaration) in definition.functions.iter().enumerate() {
            if function_index
                .insert(declaration.function.clone(), declaration.role)
                .is_some()
            {
                issues.push(issue(
                    ValidationCode::DuplicateFunction,
                    format!("functions[{index}]"),
                    format!(
                        "duplicate function `{}/{}`",
                        declaration.function.kind, declaration.function.locator
                    ),
                ));
            }
        }

        let mut state_index = HashMap::new();
        for (index, state) in definition.states.iter().enumerate() {
            if state_index.insert(state.id.clone(), index).is_some() {
                issues.push(issue(
                    ValidationCode::DuplicateState,
                    format!("states[{index}].id"),
                    format!("duplicate state `{}`", state.id),
                ));
            }
        }

        if !state_index.contains_key(&definition.initial) {
            issues.push(issue(
                ValidationCode::MissingInitialState,
                "initial",
                format!("initial state `{}` does not exist", definition.initial),
            ));
        }

        for (state_position, state) in definition.states.iter().enumerate() {
            validate_state(
                state,
                state_position,
                &state_index,
                &function_index,
                &mut issues,
            );
        }

        if !issues.is_empty() {
            return Err(ValidationErrors { issues });
        }

        let trigger_index = definition
            .states
            .iter()
            .map(|state| {
                let mut triggers: HashMap<Trigger, Vec<usize>> = HashMap::new();
                for (index, transition) in state.transitions.iter().enumerate() {
                    triggers
                        .entry(transition.trigger.clone())
                        .or_default()
                        .push(index);
                }
                triggers
            })
            .collect();

        let warnings = unreachable_warnings(&definition, &state_index);
        for warning in &warnings {
            let PreparationWarning::UnreachableState { state } = warning;
            warn!(state = %state, "machine definition contains an unreachable state");
        }

        Ok(Self {
            definition: Arc::new(definition),
            state_index,
            trigger_index,
            function_index,
            warnings,
        })
    }

    #[must_use]
    pub fn definition(&self) -> &MachineDefinition {
        &self.definition
    }

    #[must_use]
    pub fn warnings(&self) -> &[PreparationWarning] {
        &self.warnings
    }

    #[must_use]
    pub fn state(&self, id: &StateId) -> Option<&StateDefinition> {
        self.state_index
            .get(id)
            .map(|index| &self.definition.states[*index])
    }

    #[must_use]
    pub fn transition_indexes(&self, state: &StateId, trigger: &Trigger) -> &[usize] {
        let Some(state_index) = self.state_index.get(state) else {
            return &[];
        };
        self.trigger_index[*state_index]
            .get(trigger)
            .map_or(&[], Vec::as_slice)
    }

    #[must_use]
    pub fn function_role(&self, function: &FunctionRef) -> Option<FunctionRole> {
        self.function_index.get(function).copied()
    }
}

fn validate_state(
    state: &StateDefinition,
    state_position: usize,
    state_index: &HashMap<StateId, usize>,
    function_index: &HashMap<FunctionRef, FunctionRole>,
    issues: &mut Vec<ValidationIssue>,
) {
    let path = format!("states[{state_position}]");
    if state.terminal.is_some() && !state.transitions.is_empty() {
        issues.push(issue(
            ValidationCode::TerminalStateHasTransitions,
            format!("{path}.transitions"),
            "terminal states cannot declare outgoing transitions",
        ));
    }

    validate_function(
        state.entry.as_ref(),
        FunctionRole::Callback,
        format!("{path}.entry"),
        function_index,
        issues,
    );
    validate_function(
        state.exit.as_ref(),
        FunctionRole::Callback,
        format!("{path}.exit"),
        function_index,
        issues,
    );

    for (transition_position, transition) in state.transitions.iter().enumerate() {
        validate_transition(
            transition,
            &format!("{path}.transitions[{transition_position}]"),
            state_index,
            function_index,
            issues,
        );
    }
}

fn validate_transition(
    transition: &TransitionDefinition,
    path: &str,
    state_index: &HashMap<StateId, usize>,
    function_index: &HashMap<FunctionRef, FunctionRole>,
    issues: &mut Vec<ValidationIssue>,
) {
    if !state_index.contains_key(&transition.target) {
        issues.push(issue(
            ValidationCode::MissingTarget,
            format!("{path}.target"),
            format!("target state `{}` does not exist", transition.target),
        ));
    }
    if let Some(target) = &transition.failure_target {
        if !state_index.contains_key(target) {
            issues.push(issue(
                ValidationCode::MissingTarget,
                format!("{path}.failure-target"),
                format!("failure target state `{target}` does not exist"),
            ));
        }
        if transition.action.is_none() {
            issues.push(issue(
                ValidationCode::FailureTargetWithoutAction,
                format!("{path}.failure-target"),
                "a failure target requires an action",
            ));
        }
    }

    validate_function(
        transition.guard.as_ref(),
        FunctionRole::Guard,
        format!("{path}.guard"),
        function_index,
        issues,
    );
    validate_function(
        transition.action.as_ref(),
        FunctionRole::Action,
        format!("{path}.action"),
        function_index,
        issues,
    );
}

fn validate_function(
    function: Option<&FunctionRef>,
    expected: FunctionRole,
    path: String,
    function_index: &HashMap<FunctionRef, FunctionRole>,
    issues: &mut Vec<ValidationIssue>,
) {
    let Some(function) = function else {
        return;
    };
    match function_index.get(function) {
        None => issues.push(issue(
            ValidationCode::MissingFunction,
            path,
            format!(
                "function `{}/{}` is not declared",
                function.kind, function.locator
            ),
        )),
        Some(actual) if *actual != expected => issues.push(issue(
            ValidationCode::FunctionRoleMismatch,
            path,
            format!("expected {expected:?} function but declaration is {actual:?}"),
        )),
        Some(_) => {}
    }
}

fn unreachable_warnings(
    definition: &MachineDefinition,
    state_index: &HashMap<StateId, usize>,
) -> Vec<PreparationWarning> {
    if !state_index.contains_key(&definition.initial) {
        return Vec::new();
    }

    let mut reachable = HashSet::new();
    let mut queue = VecDeque::from([definition.initial.clone()]);
    while let Some(state_id) = queue.pop_front() {
        if !reachable.insert(state_id.clone()) {
            continue;
        }
        let state = &definition.states[state_index[&state_id]];
        for transition in &state.transitions {
            queue.push_back(transition.target.clone());
            if let Some(failure_target) = &transition.failure_target {
                queue.push_back(failure_target.clone());
            }
        }
    }

    definition
        .states
        .iter()
        .filter(|state| !reachable.contains(&state.id))
        .map(|state| PreparationWarning::UnreachableState {
            state: state.id.clone(),
        })
        .collect()
}

fn check_count(
    actual: usize,
    maximum: usize,
    path: &'static str,
    issues: &mut Vec<ValidationIssue>,
) {
    if actual > maximum {
        issues.push(issue(
            ValidationCode::LimitExceeded,
            path,
            format!("contains {actual} items but the configured maximum is {maximum}"),
        ));
    }
}

fn issue(
    code: ValidationCode,
    path: impl Into<String>,
    message: impl Into<String>,
) -> ValidationIssue {
    ValidationIssue {
        code,
        path: path.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ActionKind, EventName, FunctionDeclaration, FunctionId, MachineId, StateDefinition,
        TransitionDefinition,
    };

    fn id<T: TryFrom<&'static str>>(value: &'static str) -> T
    where
        T::Error: fmt::Debug,
    {
        T::try_from(value).unwrap()
    }

    fn function(name: &'static str, role: FunctionRole) -> FunctionDeclaration {
        FunctionDeclaration {
            function: FunctionRef {
                kind: ActionKind::wasm_component(),
                locator: id::<FunctionId>(name),
            },
            role,
        }
    }

    #[test]
    fn validation_aggregates_structural_issues() {
        let missing = FunctionRef::wasm(id("missing"));
        let definition = MachineDefinition {
            id: id::<MachineId>("broken"),
            initial: id("absent"),
            functions: vec![
                function("action", FunctionRole::Action),
                function("action", FunctionRole::Action),
            ],
            states: vec![
                StateDefinition {
                    id: id("same"),
                    entry: Some(missing),
                    exit: None,
                    terminal: None,
                    transitions: vec![TransitionDefinition {
                        trigger: Trigger::Event(id::<EventName>("go")),
                        guard: None,
                        action: None,
                        target: id("nowhere"),
                        failure_target: Some(id("also-nowhere")),
                    }],
                },
                StateDefinition {
                    id: id("same"),
                    entry: None,
                    exit: None,
                    terminal: None,
                    transitions: Vec::new(),
                },
            ],
        };

        let errors = ValidatedMachine::new(definition, &LoadLimits::default()).unwrap_err();
        let codes: HashSet<_> = errors.issues.iter().map(|issue| issue.code).collect();
        assert!(codes.contains(&ValidationCode::DuplicateState));
        assert!(codes.contains(&ValidationCode::DuplicateFunction));
        assert!(codes.contains(&ValidationCode::MissingInitialState));
        assert!(codes.contains(&ValidationCode::MissingFunction));
        assert!(codes.contains(&ValidationCode::MissingTarget));
        assert!(codes.contains(&ValidationCode::FailureTargetWithoutAction));
    }

    #[test]
    fn definition_preserves_transition_order_and_warns_for_unreachable_states() {
        let definition = MachineDefinition {
            id: id::<MachineId>("ordered"),
            initial: id("start"),
            functions: Vec::new(),
            states: vec![
                StateDefinition {
                    id: id("start"),
                    entry: None,
                    exit: None,
                    terminal: None,
                    transitions: vec![
                        TransitionDefinition {
                            trigger: Trigger::Event(id::<EventName>("go")),
                            guard: None,
                            action: None,
                            target: id("done"),
                            failure_target: None,
                        },
                        TransitionDefinition {
                            trigger: Trigger::Event(id::<EventName>("go")),
                            guard: None,
                            action: None,
                            target: id("done"),
                            failure_target: None,
                        },
                    ],
                },
                StateDefinition {
                    id: id("done"),
                    entry: None,
                    exit: None,
                    terminal: Some(crate::TerminalKind::Completed),
                    transitions: Vec::new(),
                },
                StateDefinition {
                    id: id("orphan"),
                    entry: None,
                    exit: None,
                    terminal: None,
                    transitions: Vec::new(),
                },
            ],
        };

        let machine = ValidatedMachine::new(definition, &LoadLimits::default()).unwrap();
        assert_eq!(
            machine.transition_indexes(
                &id::<StateId>("start"),
                &Trigger::Event(id::<EventName>("go"))
            ),
            &[0, 1]
        );
        assert_eq!(machine.warnings().len(), 1);
    }
}
