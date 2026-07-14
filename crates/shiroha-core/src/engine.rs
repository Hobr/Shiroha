use std::collections::VecDeque;
use std::sync::Arc;

use tracing::{debug, info, instrument, warn};

use crate::{
    ActionOutcome, BusinessFailureRecord, DispatchError, FailureRecord, FunctionExecutor,
    GuardInput, HookEffects, HookInput, HostInput, InstanceId, Lifecycle, MachineSnapshot,
    PayloadEnvelope, ResourceLimitKind, RunOutcome, RunReport, RuntimeFault, RuntimeFaultKind,
    RuntimeLimits, StartError, StateId, StepOutcome, StepRecord, UnhandledInput, ValidatedMachine,
};

pub struct MachineInstance {
    definition: Arc<ValidatedMachine>,
    snapshot: MachineSnapshot,
    pending: VecDeque<HostInput>,
    executor: Box<dyn FunctionExecutor>,
    limits: RuntimeLimits,
}

impl MachineInstance {
    #[instrument(name = "shiroha.start", skip_all, fields(machine = %definition.definition().id))]
    pub async fn start(
        definition: Arc<ValidatedMachine>,
        mut executor: Box<dyn FunctionExecutor>,
        initial_context: PayloadEnvelope,
        limits: RuntimeLimits,
    ) -> Result<Self, StartError> {
        let initial = definition.definition().initial.clone();
        if let Err(error) = limits.validate() {
            return Err(start_error(
                initial,
                initial_context,
                RuntimeFault::new(RuntimeFaultKind::Host, error.to_string()),
            ));
        }
        if payload_exceeds_limits(&initial_context, &limits) {
            return Err(start_error(
                initial,
                initial_context,
                limit_fault(
                    ResourceLimitKind::Payload,
                    "initial context exceeds configured payload limits",
                ),
            ));
        }

        let initial_state = definition
            .state(&initial)
            .expect("validated machine must contain its initial state")
            .clone();
        let mut staged_context = initial_context.clone();
        let mut staged_events = Vec::new();

        if let Some(entry) = &initial_state.entry {
            let effects = executor
                .invoke_callback(
                    entry,
                    HookInput {
                        source_state: initial.clone(),
                        target_state: Some(initial.clone()),
                        context: staged_context.clone(),
                        input: HostInput::Start,
                    },
                    &limits.invocation,
                )
                .await
                .map_err(|fault| {
                    start_error(
                        initial.clone(),
                        initial_context.clone(),
                        bounded_runtime_fault(fault.with_external_effects_possible(true), &limits),
                    )
                })?;
            apply_effects(&mut staged_context, &mut staged_events, effects, &limits).map_err(
                |fault| {
                    start_error(
                        initial.clone(),
                        initial_context.clone(),
                        fault.with_external_effects_possible(true),
                    )
                },
            )?;
        }

        let lifecycle = Lifecycle::from_terminal(initial_state.terminal, &initial);
        let machine_id = definition.definition().id.clone();
        let mut instance = Self {
            definition,
            snapshot: MachineSnapshot {
                machine_id,
                instance_id: InstanceId::next(),
                sequence: 0,
                state: initial,
                context: staged_context,
                lifecycle,
            },
            pending: staged_events.into_iter().map(HostInput::Event).collect(),
            executor,
            limits,
        };

        if instance.snapshot.lifecycle.is_active() && !instance.pending.is_empty() {
            let _ = instance.run_pending().await;
        } else if !instance.snapshot.lifecycle.is_active() {
            instance.pending.clear();
        }

        Ok(instance)
    }

    pub fn restore(
        definition: Arc<ValidatedMachine>,
        snapshot: MachineSnapshot,
        executor: Box<dyn FunctionExecutor>,
        limits: RuntimeLimits,
    ) -> Result<Self, DispatchError> {
        limits.validate()?;
        if snapshot.machine_id != definition.definition().id {
            return Err(DispatchError::SnapshotMachineMismatch {
                expected: definition.definition().id.clone(),
                actual: snapshot.machine_id,
            });
        }
        if payload_exceeds_limits(&snapshot.context, &limits)
            || lifecycle_payload_exceeds_limits(&snapshot.lifecycle, &limits)
        {
            return Err(DispatchError::PayloadTooLarge);
        }
        if definition.state(&snapshot.state).is_none() {
            return Err(DispatchError::NotActive(snapshot.lifecycle));
        }
        Ok(Self {
            definition,
            snapshot,
            pending: VecDeque::new(),
            executor,
            limits,
        })
    }

    #[must_use]
    pub fn snapshot(&self) -> &MachineSnapshot {
        &self.snapshot
    }

    #[must_use]
    pub fn definition(&self) -> &ValidatedMachine {
        &self.definition
    }

    pub fn replace_executor(&mut self, executor: Box<dyn FunctionExecutor>) {
        self.executor = executor;
    }

    #[instrument(
        name = "shiroha.dispatch",
        skip_all,
        fields(
            machine = %self.definition.definition().id,
            instance = %self.snapshot.instance_id,
            state = %self.snapshot.state,
            sequence = self.snapshot.sequence,
        )
    )]
    pub async fn dispatch(&mut self, input: HostInput) -> Result<RunReport, DispatchError> {
        if !self.snapshot.lifecycle.is_active() {
            return Err(DispatchError::NotActive(self.snapshot.lifecycle.clone()));
        }
        if matches!(input, HostInput::Start) {
            return Err(DispatchError::StartupInput);
        }
        self.limits.validate()?;
        if input
            .payload()
            .is_some_and(|payload| payload_exceeds_limits(payload, &self.limits))
        {
            let fault = limit_fault(
                ResourceLimitKind::Payload,
                "input payload exceeds max_payload_bytes",
            );
            let source = self.snapshot.state.clone();
            let sequence = self.snapshot.sequence;
            self.fail_runtime(fault.clone());
            return Ok(RunReport {
                start_sequence: sequence,
                end_sequence: sequence,
                microsteps: 1,
                steps: vec![StepRecord {
                    input,
                    source,
                    target: None,
                    sequence,
                    outcome: StepOutcome::Fault(fault),
                }],
                unhandled: Vec::new(),
                outcome: outcome(&self.snapshot.lifecycle),
            });
        }

        self.pending.push_back(input);
        Ok(self.run_pending().await)
    }

    async fn run_pending(&mut self) -> RunReport {
        let start_sequence = self.snapshot.sequence;
        let mut steps = Vec::new();
        let mut unhandled = Vec::new();
        let mut microsteps = 0;

        while self.snapshot.lifecycle.is_active() && !self.pending.is_empty() {
            if microsteps >= self.limits.max_microsteps {
                let fault = limit_fault(
                    ResourceLimitKind::Microsteps,
                    "run-to-completion exceeded max_microsteps",
                );
                self.fail_runtime(fault);
                self.pending.clear();
                break;
            }

            let input = self
                .pending
                .pop_front()
                .expect("queue was checked as non-empty");
            microsteps += 1;
            let source = self.snapshot.state.clone();

            match self.process_input(input.clone()).await {
                Ok(record) => {
                    if matches!(record.outcome, StepOutcome::Unhandled) {
                        unhandled.push(UnhandledInput {
                            state: source,
                            input: input.clone(),
                        });
                        debug!("input was not handled by the active state");
                    }
                    steps.push(record);
                }
                Err(fault) => {
                    let fault = bounded_runtime_fault(fault, &self.limits);
                    warn!(kind = %fault.kind, "state-machine step failed");
                    self.fail_runtime(fault.clone());
                    steps.push(StepRecord {
                        input,
                        source,
                        target: None,
                        sequence: self.snapshot.sequence,
                        outcome: StepOutcome::Fault(fault),
                    });
                }
            }
        }

        if !self.snapshot.lifecycle.is_active() {
            self.pending.clear();
        }

        RunReport {
            start_sequence,
            end_sequence: self.snapshot.sequence,
            microsteps,
            steps,
            unhandled,
            outcome: outcome(&self.snapshot.lifecycle),
        }
    }

    #[instrument(
        name = "shiroha.step",
        skip_all,
        fields(state = %self.snapshot.state, sequence = self.snapshot.sequence)
    )]
    async fn process_input(&mut self, input: HostInput) -> Result<StepRecord, RuntimeFault> {
        let source_id = self.snapshot.state.clone();
        let source_state = self
            .definition
            .state(&source_id)
            .expect("snapshot state must belong to the validated definition")
            .clone();
        let trigger = input
            .trigger()
            .expect("startup input is rejected before queue processing");
        let candidates = self
            .definition
            .transition_indexes(&source_id, &trigger)
            .to_vec();

        let mut selected = None;
        for transition_index in candidates {
            let transition = source_state.transitions[transition_index].clone();
            let eligible = if let Some(guard) = &transition.guard {
                self.executor
                    .evaluate_guard(
                        guard,
                        GuardInput {
                            source_state: source_id.clone(),
                            context: self.snapshot.context.clone(),
                            input: input.clone(),
                        },
                        &self.limits.invocation,
                    )
                    .await?
            } else {
                true
            };
            if eligible {
                selected = Some((transition_index, transition));
                break;
            }
        }

        let Some((transition_index, transition)) = selected else {
            if let HostInput::Cancel(cancel) = &input {
                self.snapshot.sequence = self.snapshot.sequence.saturating_add(1);
                self.snapshot.lifecycle = Lifecycle::Cancelled {
                    reason: cancel.reason.clone(),
                };
                return Ok(StepRecord {
                    input,
                    source: source_id,
                    target: None,
                    sequence: self.snapshot.sequence,
                    outcome: StepOutcome::Cancelled,
                });
            }
            return Ok(StepRecord {
                input,
                source: source_id,
                target: None,
                sequence: self.snapshot.sequence,
                outcome: StepOutcome::Unhandled,
            });
        };

        let mut staged_context = self.snapshot.context.clone();
        let mut staged_events = Vec::new();
        if let Some(exit) = &source_state.exit {
            let effects = self
                .executor
                .invoke_callback(
                    exit,
                    HookInput {
                        source_state: source_id.clone(),
                        target_state: Some(transition.target.clone()),
                        context: staged_context.clone(),
                        input: input.clone(),
                    },
                    &self.limits.invocation,
                )
                .await
                .map_err(|fault| fault.with_external_effects_possible(true))?;
            apply_effects(
                &mut staged_context,
                &mut staged_events,
                effects,
                &self.limits,
            )
            .map_err(|fault| fault.with_external_effects_possible(true))?;
        }

        let mut business_failure = None;
        let target = if let Some(action) = &transition.action {
            let outcome = self
                .executor
                .invoke_action(
                    action,
                    HookInput {
                        source_state: source_id.clone(),
                        target_state: Some(transition.target.clone()),
                        context: staged_context.clone(),
                        input: input.clone(),
                    },
                    &self.limits.invocation,
                )
                .await
                .map_err(|fault| fault.with_external_effects_possible(true))?;

            match outcome {
                ActionOutcome::Succeeded(effects) => {
                    apply_effects(
                        &mut staged_context,
                        &mut staged_events,
                        effects,
                        &self.limits,
                    )
                    .map_err(|fault| fault.with_external_effects_possible(true))?;
                    transition.target.clone()
                }
                ActionOutcome::Failed(failure) => {
                    if failure
                        .payload
                        .as_ref()
                        .is_some_and(|payload| payload_exceeds_limits(payload, &self.limits))
                    {
                        return Err(limit_fault(
                            ResourceLimitKind::Payload,
                            "business-failure payload exceeds configured limits",
                        )
                        .with_external_effects_possible(true));
                    }
                    let record = BusinessFailureRecord {
                        code: failure.code,
                        payload: failure.payload,
                        external_effects_possible: true,
                    };
                    let Some(failure_target) = transition.failure_target.clone() else {
                        self.snapshot.lifecycle =
                            Lifecycle::Failed(FailureRecord::Business(record.clone()));
                        return Ok(StepRecord {
                            input,
                            source: source_id,
                            target: None,
                            sequence: self.snapshot.sequence,
                            outcome: StepOutcome::BusinessFailed(record),
                        });
                    };
                    apply_effects(
                        &mut staged_context,
                        &mut staged_events,
                        failure.effects,
                        &self.limits,
                    )
                    .map_err(|fault| fault.with_external_effects_possible(true))?;
                    business_failure = Some(record);
                    failure_target
                }
            }
        } else {
            transition.target.clone()
        };

        let target_state = self
            .definition
            .state(&target)
            .expect("validated transition target must exist")
            .clone();
        if let Some(entry) = &target_state.entry {
            let effects = self
                .executor
                .invoke_callback(
                    entry,
                    HookInput {
                        source_state: source_id.clone(),
                        target_state: Some(target.clone()),
                        context: staged_context.clone(),
                        input: input.clone(),
                    },
                    &self.limits.invocation,
                )
                .await
                .map_err(|fault| fault.with_external_effects_possible(true))?;
            apply_effects(
                &mut staged_context,
                &mut staged_events,
                effects,
                &self.limits,
            )
            .map_err(|fault| fault.with_external_effects_possible(true))?;
        }

        self.snapshot.sequence = self.snapshot.sequence.saturating_add(1);
        self.snapshot.state = target.clone();
        self.snapshot.context = staged_context;
        self.snapshot.lifecycle = Lifecycle::from_terminal(target_state.terminal, &target);
        if self.snapshot.lifecycle.is_active() {
            self.pending
                .extend(staged_events.into_iter().map(HostInput::Event));
        }
        info!(
            source = %source_id,
            target = %target,
            transition_index,
            "state transition committed"
        );

        Ok(StepRecord {
            input,
            source: source_id,
            target: Some(target),
            sequence: self.snapshot.sequence,
            outcome: StepOutcome::Transitioned {
                transition_index,
                business_failure,
            },
        })
    }

    fn fail_runtime(&mut self, fault: RuntimeFault) {
        self.snapshot.lifecycle = Lifecycle::Failed(FailureRecord::Runtime(fault));
    }
}

fn apply_effects(
    staged_context: &mut PayloadEnvelope,
    staged_events: &mut Vec<crate::Event>,
    effects: HookEffects,
    limits: &RuntimeLimits,
) -> Result<(), RuntimeFault> {
    if effects.events.len() > limits.max_events_per_hook {
        return Err(limit_fault(
            ResourceLimitKind::Events,
            "hook emitted more events than max_events_per_hook",
        ));
    }
    if effects
        .replacement_context
        .as_ref()
        .is_some_and(|context| payload_exceeds_limits(context, limits))
        || effects.events.iter().any(|event| {
            event
                .payload
                .as_ref()
                .is_some_and(|payload| payload_exceeds_limits(payload, limits))
        })
    {
        return Err(limit_fault(
            ResourceLimitKind::Payload,
            "hook output exceeds max_payload_bytes",
        ));
    }

    if let Some(context) = effects.replacement_context {
        *staged_context = context;
    }
    staged_events.extend(effects.events);
    Ok(())
}

fn payload_exceeds_limits(payload: &PayloadEnvelope, limits: &RuntimeLimits) -> bool {
    payload.data().len() > limits.max_payload_bytes
        || payload.content_type().len() > limits.max_metadata_bytes
        || payload
            .schema_id()
            .is_some_and(|schema_id| schema_id.len() > limits.max_metadata_bytes)
}

fn lifecycle_payload_exceeds_limits(lifecycle: &Lifecycle, limits: &RuntimeLimits) -> bool {
    let payload = match lifecycle {
        Lifecycle::Failed(FailureRecord::Business(failure)) => failure.payload.as_ref(),
        Lifecycle::Failed(FailureRecord::Runtime(fault)) => fault.payload.as_ref(),
        Lifecycle::Cancelled { reason } => reason.as_ref(),
        Lifecycle::Active
        | Lifecycle::Completed
        | Lifecycle::Failed(FailureRecord::Terminal { .. }) => None,
    };
    payload.is_some_and(|payload| payload_exceeds_limits(payload, limits))
}

fn bounded_runtime_fault(fault: RuntimeFault, limits: &RuntimeLimits) -> RuntimeFault {
    if fault
        .payload
        .as_ref()
        .is_some_and(|payload| payload_exceeds_limits(payload, limits))
    {
        limit_fault(
            ResourceLimitKind::Payload,
            "runtime-fault payload exceeds configured limits",
        )
        .with_external_effects_possible(fault.external_effects_possible)
    } else {
        fault
    }
}

fn start_error(
    attempted_state: StateId,
    attempted_context: PayloadEnvelope,
    fault: RuntimeFault,
) -> StartError {
    StartError {
        attempted_state,
        attempted_context,
        fault,
    }
}

fn limit_fault(kind: ResourceLimitKind, message: &'static str) -> RuntimeFault {
    RuntimeFault::new(RuntimeFaultKind::ResourceLimit(kind), message)
}

fn outcome(lifecycle: &Lifecycle) -> RunOutcome {
    match lifecycle {
        Lifecycle::Active => RunOutcome::Quiescent,
        Lifecycle::Completed => RunOutcome::Completed,
        Lifecycle::Failed(failure) => RunOutcome::Failed(failure.clone()),
        Lifecycle::Cancelled { .. } => RunOutcome::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::fmt;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;

    use super::*;
    use crate::{
        ActionKind, BusinessFailure, CancelInput, Event, EventName, FunctionDeclaration,
        FunctionId, FunctionRef, FunctionRole, LoadLimits, MachineDefinition, MachineId,
        StateDefinition, TerminalKind, TimeoutInput, TimeoutKey, TransitionDefinition, Trigger,
    };

    #[derive(Default)]
    struct MockExecutor {
        log: Arc<Mutex<Vec<String>>>,
        guards: HashMap<String, bool>,
        callbacks: HashMap<String, HookEffects>,
        actions: HashMap<String, ActionOutcome>,
        faults: HashMap<String, RuntimeFault>,
    }

    impl MockExecutor {
        fn shared_log(&self) -> Arc<Mutex<Vec<String>>> {
            Arc::clone(&self.log)
        }

        fn guard(mut self, name: &str, value: bool) -> Self {
            self.guards.insert(name.to_owned(), value);
            self
        }

        fn callback(mut self, name: &str, effects: HookEffects) -> Self {
            self.callbacks.insert(name.to_owned(), effects);
            self
        }

        fn action(mut self, name: &str, outcome: ActionOutcome) -> Self {
            self.actions.insert(name.to_owned(), outcome);
            self
        }

        fn fault(mut self, name: &str) -> Self {
            self.faults.insert(
                name.to_owned(),
                RuntimeFault::new(RuntimeFaultKind::Guest, format!("{name} failed")),
            );
            self
        }

        fn detailed_fault(mut self, name: &str, fault: RuntimeFault) -> Self {
            self.faults.insert(name.to_owned(), fault);
            self
        }

        fn record(&self, kind: &str, function: &FunctionRef, context: &PayloadEnvelope) {
            let context = String::from_utf8_lossy(context.data());
            self.log
                .lock()
                .unwrap()
                .push(format!("{kind}:{}:{context}", function.locator.as_str()));
        }

        fn possible_fault(&self, function: &FunctionRef) -> Result<(), RuntimeFault> {
            self.faults
                .get(function.locator.as_str())
                .cloned()
                .map_or(Ok(()), Err)
        }
    }

    #[async_trait]
    impl FunctionExecutor for MockExecutor {
        async fn evaluate_guard(
            &mut self,
            function: &FunctionRef,
            input: GuardInput,
            _: &crate::InvocationLimits,
        ) -> Result<bool, RuntimeFault> {
            self.record("guard", function, &input.context);
            self.possible_fault(function)?;
            Ok(self
                .guards
                .get(function.locator.as_str())
                .copied()
                .unwrap_or(true))
        }

        async fn invoke_callback(
            &mut self,
            function: &FunctionRef,
            input: HookInput,
            _: &crate::InvocationLimits,
        ) -> Result<HookEffects, RuntimeFault> {
            self.record("callback", function, &input.context);
            self.possible_fault(function)?;
            Ok(self
                .callbacks
                .get(function.locator.as_str())
                .cloned()
                .unwrap_or_default())
        }

        async fn invoke_action(
            &mut self,
            function: &FunctionRef,
            input: HookInput,
            _: &crate::InvocationLimits,
        ) -> Result<ActionOutcome, RuntimeFault> {
            self.record("action", function, &input.context);
            self.possible_fault(function)?;
            Ok(self
                .actions
                .get(function.locator.as_str())
                .cloned()
                .unwrap_or_else(|| ActionOutcome::Succeeded(HookEffects::default())))
        }
    }

    fn id<T: TryFrom<&'static str>>(value: &'static str) -> T
    where
        T::Error: fmt::Debug,
    {
        T::try_from(value).unwrap()
    }

    fn function(name: &'static str) -> FunctionRef {
        FunctionRef::wasm(id::<FunctionId>(name))
    }

    fn declaration(name: &'static str, role: FunctionRole) -> FunctionDeclaration {
        FunctionDeclaration {
            function: function(name),
            role,
        }
    }

    fn event(name: &'static str) -> HostInput {
        HostInput::Event(Event::new(id::<EventName>(name), None))
    }

    fn emitted(name: &'static str) -> Event {
        Event::new(id::<EventName>(name), None)
    }

    fn context(value: &'static str) -> PayloadEnvelope {
        PayloadEnvelope::json(value.as_bytes())
    }

    fn effects(replacement: Option<&'static str>, events: Vec<Event>) -> HookEffects {
        HookEffects {
            replacement_context: replacement.map(context),
            events,
        }
    }

    fn transition(
        event: &'static str,
        guard: Option<&'static str>,
        action: Option<&'static str>,
        target: &'static str,
        failure_target: Option<&'static str>,
    ) -> TransitionDefinition {
        TransitionDefinition {
            trigger: Trigger::Event(id::<EventName>(event)),
            guard: guard.map(function),
            action: action.map(function),
            target: id::<StateId>(target),
            failure_target: failure_target.map(id::<StateId>),
        }
    }

    fn state(
        name: &'static str,
        entry: Option<&'static str>,
        exit: Option<&'static str>,
        terminal: Option<TerminalKind>,
        transitions: Vec<TransitionDefinition>,
    ) -> StateDefinition {
        StateDefinition {
            id: id::<StateId>(name),
            entry: entry.map(function),
            exit: exit.map(function),
            terminal,
            transitions,
        }
    }

    fn machine(
        functions: Vec<FunctionDeclaration>,
        states: Vec<StateDefinition>,
    ) -> Arc<ValidatedMachine> {
        Arc::new(
            ValidatedMachine::new(
                MachineDefinition {
                    id: id::<MachineId>("test-machine"),
                    initial: states[0].id.clone(),
                    functions,
                    states,
                },
                &LoadLimits::default(),
            )
            .unwrap(),
        )
    }

    fn log_values(log: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
        log.lock().unwrap().clone()
    }

    #[tokio::test]
    async fn engine_uses_fixed_lifecycle_order_and_staged_context() {
        let definition = machine(
            vec![
                declaration("enter-a", FunctionRole::Callback),
                declaration("exit-a", FunctionRole::Callback),
                declaration("guard", FunctionRole::Guard),
                declaration("action", FunctionRole::Action),
                declaration("enter-b", FunctionRole::Callback),
            ],
            vec![
                state(
                    "a",
                    Some("enter-a"),
                    Some("exit-a"),
                    None,
                    vec![transition("go", Some("guard"), Some("action"), "b", None)],
                ),
                state("b", Some("enter-b"), None, None, Vec::new()),
            ],
        );
        let executor = MockExecutor::default()
            .guard("guard", true)
            .callback("exit-a", effects(Some("after-exit"), Vec::new()))
            .action(
                "action",
                ActionOutcome::Succeeded(effects(Some("after-action"), Vec::new())),
            )
            .callback("enter-b", effects(Some("after-entry"), Vec::new()));
        let log = executor.shared_log();
        let mut instance = MachineInstance::start(
            definition,
            Box::new(executor),
            context("initial"),
            RuntimeLimits::default(),
        )
        .await
        .unwrap();
        log.lock().unwrap().clear();

        let report = instance.dispatch(event("go")).await.unwrap();
        assert_eq!(
            log_values(&log),
            [
                "guard:guard:initial",
                "callback:exit-a:initial",
                "action:action:after-exit",
                "callback:enter-b:after-action",
            ]
        );
        assert_eq!(instance.snapshot.state, id::<StateId>("b"));
        assert_eq!(instance.snapshot.context, context("after-entry"));
        assert_eq!(instance.snapshot.sequence, 1);
        assert_eq!(report.outcome, RunOutcome::Quiescent);
    }

    #[tokio::test]
    async fn engine_evaluates_guards_in_declaration_order() {
        let definition = machine(
            vec![
                declaration("first", FunctionRole::Guard),
                declaration("second", FunctionRole::Guard),
            ],
            vec![
                state(
                    "a",
                    None,
                    None,
                    None,
                    vec![
                        transition("go", Some("first"), None, "wrong", None),
                        transition("go", Some("second"), None, "right", None),
                    ],
                ),
                state("wrong", None, None, None, Vec::new()),
                state("right", None, None, None, Vec::new()),
            ],
        );
        let executor = MockExecutor::default()
            .guard("first", false)
            .guard("second", true);
        let log = executor.shared_log();
        let mut instance = MachineInstance::start(
            definition,
            Box::new(executor),
            context("initial"),
            RuntimeLimits::default(),
        )
        .await
        .unwrap();

        instance.dispatch(event("go")).await.unwrap();
        assert_eq!(
            log_values(&log),
            ["guard:first:initial", "guard:second:initial"]
        );
        assert_eq!(instance.snapshot.state, id::<StateId>("right"));
    }

    #[tokio::test]
    async fn self_transition_exits_and_reenters_the_same_state() {
        let definition = machine(
            vec![
                declaration("enter", FunctionRole::Callback),
                declaration("exit", FunctionRole::Callback),
            ],
            vec![state(
                "a",
                Some("enter"),
                Some("exit"),
                None,
                vec![transition("loop", None, None, "a", None)],
            )],
        );
        let executor = MockExecutor::default();
        let log = executor.shared_log();
        let mut instance = MachineInstance::start(
            definition,
            Box::new(executor),
            context("initial"),
            RuntimeLimits::default(),
        )
        .await
        .unwrap();
        log.lock().unwrap().clear();

        instance.dispatch(event("loop")).await.unwrap();
        assert_eq!(
            log_values(&log),
            ["callback:exit:initial", "callback:enter:initial"]
        );
        assert_eq!(instance.snapshot.state, id::<StateId>("a"));
        assert_eq!(instance.snapshot.sequence, 1);
    }

    #[tokio::test]
    async fn failure_target_commits_failure_effects_then_entry() {
        let definition = machine(
            vec![
                declaration("action", FunctionRole::Action),
                declaration("enter-failure", FunctionRole::Callback),
            ],
            vec![
                state(
                    "a",
                    None,
                    None,
                    None,
                    vec![transition(
                        "go",
                        None,
                        Some("action"),
                        "ok",
                        Some("failure"),
                    )],
                ),
                state("ok", None, None, None, Vec::new()),
                state("failure", Some("enter-failure"), None, None, Vec::new()),
            ],
        );
        let executor = MockExecutor::default()
            .action(
                "action",
                ActionOutcome::Failed(BusinessFailure {
                    code: "declined".to_owned(),
                    payload: None,
                    effects: effects(Some("failure-effects"), Vec::new()),
                }),
            )
            .callback("enter-failure", effects(Some("failure-entry"), Vec::new()));
        let log = executor.shared_log();
        let mut instance = MachineInstance::start(
            definition,
            Box::new(executor),
            context("initial"),
            RuntimeLimits::default(),
        )
        .await
        .unwrap();

        let report = instance.dispatch(event("go")).await.unwrap();
        assert_eq!(instance.snapshot.state, id::<StateId>("failure"));
        assert_eq!(instance.snapshot.context, context("failure-entry"));
        assert_eq!(
            log_values(&log),
            [
                "action:action:initial",
                "callback:enter-failure:failure-effects"
            ]
        );
        assert!(matches!(
            report.steps[0].outcome,
            StepOutcome::Transitioned {
                business_failure: Some(_),
                ..
            }
        ));
    }

    #[tokio::test]
    async fn business_failure_without_target_discards_effects_and_fails() {
        let definition = machine(
            vec![declaration("action", FunctionRole::Action)],
            vec![
                state(
                    "a",
                    None,
                    None,
                    None,
                    vec![transition("go", None, Some("action"), "ok", None)],
                ),
                state("ok", None, None, None, Vec::new()),
            ],
        );
        let executor = MockExecutor::default().action(
            "action",
            ActionOutcome::Failed(BusinessFailure {
                code: "declined".to_owned(),
                payload: None,
                effects: effects(Some("must-not-commit"), vec![emitted("later")]),
            }),
        );
        let mut instance = MachineInstance::start(
            definition,
            Box::new(executor),
            context("committed"),
            RuntimeLimits::default(),
        )
        .await
        .unwrap();

        let report = instance.dispatch(event("go")).await.unwrap();
        assert_eq!(instance.snapshot.state, id::<StateId>("a"));
        assert_eq!(instance.snapshot.context, context("committed"));
        assert_eq!(instance.snapshot.sequence, 0);
        assert!(matches!(
            report.outcome,
            RunOutcome::Failed(FailureRecord::Business(_))
        ));
        assert!(instance.pending.is_empty());
    }

    #[tokio::test]
    async fn startup_entry_fault_never_creates_a_committed_instance() {
        let definition = machine(
            vec![declaration("enter", FunctionRole::Callback)],
            vec![state("a", Some("enter"), None, None, Vec::new())],
        );
        let result = MachineInstance::start(
            definition,
            Box::new(MockExecutor::default().fault("enter")),
            context("initial"),
            RuntimeLimits::default(),
        )
        .await;
        let error = match result {
            Ok(_) => panic!("startup unexpectedly committed"),
            Err(error) => error,
        };
        assert_eq!(error.attempted_state, id::<StateId>("a"));
        assert_eq!(error.attempted_context, context("initial"));
        assert!(error.fault.external_effects_possible);
    }

    #[tokio::test]
    async fn startup_replaces_an_oversized_guest_fault_payload() {
        let definition = machine(
            vec![declaration("enter", FunctionRole::Callback)],
            vec![state("a", Some("enter"), None, None, Vec::new())],
        );
        let fault = RuntimeFault::new(RuntimeFaultKind::Guest, "guest failed")
            .with_guest_details("guest-error", Some(context("too-large")));
        let limits = RuntimeLimits {
            max_payload_bytes: 4,
            ..RuntimeLimits::default()
        };
        let result = MachineInstance::start(
            definition,
            Box::new(MockExecutor::default().detailed_fault("enter", fault)),
            context("ok"),
            limits,
        )
        .await;
        let error = match result {
            Ok(_) => panic!("startup unexpectedly committed"),
            Err(error) => error,
        };
        assert!(matches!(
            error.fault,
            RuntimeFault {
                kind: RuntimeFaultKind::ResourceLimit(ResourceLimitKind::Payload),
                payload: None,
                external_effects_possible: true,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn atomic_commit_discards_staged_changes_after_runtime_fault() {
        let definition = machine(
            vec![
                declaration("action", FunctionRole::Action),
                declaration("enter-b", FunctionRole::Callback),
            ],
            vec![
                state(
                    "a",
                    None,
                    None,
                    None,
                    vec![transition("go", None, Some("action"), "b", None)],
                ),
                state("b", Some("enter-b"), None, None, Vec::new()),
            ],
        );
        let executor = MockExecutor::default()
            .action(
                "action",
                ActionOutcome::Succeeded(effects(Some("staged"), vec![emitted("later")])),
            )
            .fault("enter-b");
        let mut instance = MachineInstance::start(
            definition,
            Box::new(executor),
            context("committed"),
            RuntimeLimits::default(),
        )
        .await
        .unwrap();

        let report = instance.dispatch(event("go")).await.unwrap();
        assert_eq!(instance.snapshot.state, id::<StateId>("a"));
        assert_eq!(instance.snapshot.context, context("committed"));
        assert_eq!(instance.snapshot.sequence, 0);
        assert!(matches!(instance.snapshot.lifecycle, Lifecycle::Failed(_)));
        assert!(matches!(report.steps[0].outcome, StepOutcome::Fault(_)));
        assert!(instance.pending.is_empty());
    }

    #[tokio::test]
    async fn run_to_completion_drains_internal_events_fifo() {
        let definition = machine(
            vec![
                declaration("main", FunctionRole::Action),
                declaration("one", FunctionRole::Action),
                declaration("two", FunctionRole::Action),
                declaration("three", FunctionRole::Action),
            ],
            vec![
                state(
                    "a",
                    None,
                    None,
                    None,
                    vec![transition("go", None, Some("main"), "b", None)],
                ),
                state(
                    "b",
                    None,
                    None,
                    None,
                    vec![
                        transition("one", None, Some("one"), "b", None),
                        transition("two", None, Some("two"), "b", None),
                        transition("three", None, Some("three"), "done", None),
                    ],
                ),
                state(
                    "done",
                    None,
                    None,
                    Some(TerminalKind::Completed),
                    Vec::new(),
                ),
            ],
        );
        let executor = MockExecutor::default()
            .action(
                "main",
                ActionOutcome::Succeeded(effects(None, vec![emitted("one"), emitted("two")])),
            )
            .action(
                "one",
                ActionOutcome::Succeeded(effects(None, vec![emitted("three")])),
            );
        let log = executor.shared_log();
        let mut instance = MachineInstance::start(
            definition,
            Box::new(executor),
            context("initial"),
            RuntimeLimits::default(),
        )
        .await
        .unwrap();

        let report = instance.dispatch(event("go")).await.unwrap();
        assert_eq!(
            log_values(&log),
            [
                "action:main:initial",
                "action:one:initial",
                "action:two:initial",
                "action:three:initial",
            ]
        );
        assert_eq!(report.microsteps, 4);
        assert_eq!(report.outcome, RunOutcome::Completed);
    }

    #[tokio::test]
    async fn run_to_completion_enforces_microstep_limit() {
        let definition = machine(
            vec![declaration("loop", FunctionRole::Action)],
            vec![state(
                "a",
                None,
                None,
                None,
                vec![
                    transition("go", None, Some("loop"), "a", None),
                    transition("tick", None, Some("loop"), "a", None),
                ],
            )],
        );
        let executor = MockExecutor::default().action(
            "loop",
            ActionOutcome::Succeeded(effects(None, vec![emitted("tick")])),
        );
        let limits = RuntimeLimits {
            max_microsteps: 3,
            ..RuntimeLimits::default()
        };
        let mut instance =
            MachineInstance::start(definition, Box::new(executor), context("initial"), limits)
                .await
                .unwrap();

        let report = instance.dispatch(event("go")).await.unwrap();
        assert_eq!(instance.snapshot.sequence, 3);
        assert!(matches!(
            report.outcome,
            RunOutcome::Failed(FailureRecord::Runtime(RuntimeFault {
                kind: RuntimeFaultKind::ResourceLimit(ResourceLimitKind::Microsteps),
                ..
            }))
        ));
    }

    #[tokio::test]
    async fn engine_reports_unhandled_event_without_mutation() {
        let definition = machine(Vec::new(), vec![state("a", None, None, None, Vec::new())]);
        let mut instance = MachineInstance::start(
            definition,
            Box::new(MockExecutor::default()),
            context("initial"),
            RuntimeLimits::default(),
        )
        .await
        .unwrap();

        let report = instance.dispatch(event("unknown")).await.unwrap();
        assert_eq!(report.unhandled.len(), 1);
        assert_eq!(instance.snapshot.sequence, 0);
        assert_eq!(instance.snapshot.context, context("initial"));
        assert_eq!(instance.snapshot.lifecycle, Lifecycle::Active);
    }

    #[tokio::test]
    async fn logical_timeout_selects_its_explicit_transition() {
        let definition = machine(
            Vec::new(),
            vec![
                state(
                    "a",
                    None,
                    None,
                    None,
                    vec![TransitionDefinition {
                        trigger: Trigger::Timeout(id::<TimeoutKey>("lease")),
                        guard: None,
                        action: None,
                        target: id::<StateId>("timed-out"),
                        failure_target: None,
                    }],
                ),
                state(
                    "timed-out",
                    None,
                    None,
                    Some(TerminalKind::Completed),
                    Vec::new(),
                ),
            ],
        );
        let mut instance = MachineInstance::start(
            definition,
            Box::new(MockExecutor::default()),
            context("initial"),
            RuntimeLimits::default(),
        )
        .await
        .unwrap();

        let report = instance
            .dispatch(HostInput::Timeout(TimeoutInput {
                key: id::<TimeoutKey>("lease"),
                payload: None,
            }))
            .await
            .unwrap();
        assert_eq!(report.outcome, RunOutcome::Completed);
        assert_eq!(instance.snapshot.state, id::<StateId>("timed-out"));
    }

    #[tokio::test]
    async fn oversized_input_is_a_structured_resource_fault() {
        let definition = machine(Vec::new(), vec![state("a", None, None, None, Vec::new())]);
        let limits = RuntimeLimits {
            max_payload_bytes: 4,
            ..RuntimeLimits::default()
        };
        let mut instance = MachineInstance::start(
            definition,
            Box::new(MockExecutor::default()),
            context("ok"),
            limits,
        )
        .await
        .unwrap();

        let report = instance
            .dispatch(HostInput::Event(Event::new(
                id::<EventName>("large"),
                Some(context("too-large")),
            )))
            .await
            .unwrap();
        assert_eq!(instance.snapshot.sequence, 0);
        assert_eq!(instance.snapshot.context, context("ok"));
        assert!(matches!(
            report.outcome,
            RunOutcome::Failed(FailureRecord::Runtime(RuntimeFault {
                kind: RuntimeFaultKind::ResourceLimit(ResourceLimitKind::Payload),
                ..
            }))
        ));
    }

    #[tokio::test]
    async fn oversized_payload_metadata_is_a_structured_resource_fault() {
        let definition = machine(Vec::new(), vec![state("a", None, None, None, Vec::new())]);
        let limits = RuntimeLimits {
            max_metadata_bytes: 4,
            ..RuntimeLimits::default()
        };
        let mut instance = MachineInstance::start(
            definition,
            Box::new(MockExecutor::default()),
            PayloadEnvelope::new(b"ok".to_vec(), "json", None),
            limits,
        )
        .await
        .unwrap();

        let report = instance
            .dispatch(HostInput::Event(Event::new(
                id::<EventName>("metadata"),
                Some(PayloadEnvelope::new(Vec::new(), "text/plain", None)),
            )))
            .await
            .unwrap();
        assert!(matches!(
            report.outcome,
            RunOutcome::Failed(FailureRecord::Runtime(RuntimeFault {
                kind: RuntimeFaultKind::ResourceLimit(ResourceLimitKind::Payload),
                ..
            }))
        ));
    }

    #[tokio::test]
    async fn oversized_business_failure_payload_is_not_committed() {
        let definition = machine(
            vec![declaration("action", FunctionRole::Action)],
            vec![
                state(
                    "a",
                    None,
                    None,
                    None,
                    vec![transition("go", None, Some("action"), "done", None)],
                ),
                state(
                    "done",
                    None,
                    None,
                    Some(TerminalKind::Completed),
                    Vec::new(),
                ),
            ],
        );
        let executor = MockExecutor::default().action(
            "action",
            ActionOutcome::Failed(BusinessFailure {
                code: "declined".to_owned(),
                payload: Some(context("too-large")),
                effects: HookEffects::default(),
            }),
        );
        let limits = RuntimeLimits {
            max_payload_bytes: 4,
            ..RuntimeLimits::default()
        };
        let mut instance =
            MachineInstance::start(definition, Box::new(executor), context("ok"), limits)
                .await
                .unwrap();

        let report = instance.dispatch(event("go")).await.unwrap();
        assert_eq!(instance.snapshot.context, context("ok"));
        assert!(matches!(
            report.outcome,
            RunOutcome::Failed(FailureRecord::Runtime(RuntimeFault {
                kind: RuntimeFaultKind::ResourceLimit(ResourceLimitKind::Payload),
                ..
            }))
        ));
    }

    #[tokio::test]
    async fn oversized_guest_fault_payload_is_replaced_by_a_limit_fault() {
        let definition = machine(
            vec![declaration("action", FunctionRole::Action)],
            vec![
                state(
                    "a",
                    None,
                    None,
                    None,
                    vec![transition("go", None, Some("action"), "done", None)],
                ),
                state(
                    "done",
                    None,
                    None,
                    Some(TerminalKind::Completed),
                    Vec::new(),
                ),
            ],
        );
        let guest_fault = RuntimeFault::new(RuntimeFaultKind::Guest, "guest failed")
            .with_guest_details("guest-error", Some(context("too-large")));
        let executor = MockExecutor::default().detailed_fault("action", guest_fault);
        let limits = RuntimeLimits {
            max_payload_bytes: 4,
            ..RuntimeLimits::default()
        };
        let mut instance =
            MachineInstance::start(definition, Box::new(executor), context("ok"), limits)
                .await
                .unwrap();

        let report = instance.dispatch(event("go")).await.unwrap();
        assert!(matches!(
            report.outcome,
            RunOutcome::Failed(FailureRecord::Runtime(RuntimeFault {
                kind: RuntimeFaultKind::ResourceLimit(ResourceLimitKind::Payload),
                payload: None,
                ..
            }))
        ));
    }

    #[tokio::test]
    async fn engine_commits_unhandled_cancel_as_cancelled() {
        let definition = machine(Vec::new(), vec![state("a", None, None, None, Vec::new())]);
        let mut instance = MachineInstance::start(
            definition,
            Box::new(MockExecutor::default()),
            context("initial"),
            RuntimeLimits::default(),
        )
        .await
        .unwrap();

        let report = instance
            .dispatch(HostInput::Cancel(CancelInput {
                reason: Some(context("stop")),
            }))
            .await
            .unwrap();
        assert_eq!(report.outcome, RunOutcome::Cancelled);
        assert_eq!(instance.snapshot.sequence, 1);
        assert!(matches!(
            &instance.snapshot.lifecycle,
            Lifecycle::Cancelled { reason: Some(reason) } if reason == &context("stop")
        ));
    }

    #[tokio::test]
    async fn recreate_executor_restores_from_host_snapshot() {
        let definition = machine(
            Vec::new(),
            vec![
                state(
                    "a",
                    None,
                    None,
                    None,
                    vec![transition("first", None, None, "b", None)],
                ),
                state(
                    "b",
                    None,
                    None,
                    None,
                    vec![transition("second", None, None, "done", None)],
                ),
                state(
                    "done",
                    None,
                    None,
                    Some(TerminalKind::Completed),
                    Vec::new(),
                ),
            ],
        );
        let mut first = MachineInstance::start(
            Arc::clone(&definition),
            Box::new(MockExecutor::default()),
            context("host-owned"),
            RuntimeLimits::default(),
        )
        .await
        .unwrap();
        first.dispatch(event("first")).await.unwrap();

        let mut restored = MachineInstance::restore(
            definition,
            first.snapshot.clone(),
            Box::new(MockExecutor::default()),
            RuntimeLimits::default(),
        )
        .unwrap();
        let report = restored.dispatch(event("second")).await.unwrap();
        assert_eq!(report.outcome, RunOutcome::Completed);
        assert_eq!(restored.snapshot.context, context("host-owned"));
        assert_eq!(restored.snapshot.sequence, 2);
    }

    #[tokio::test]
    async fn restore_rejects_a_snapshot_from_another_machine() {
        let definition = machine(Vec::new(), vec![state("a", None, None, None, Vec::new())]);
        let instance = MachineInstance::start(
            Arc::clone(&definition),
            Box::new(MockExecutor::default()),
            context("host-owned"),
            RuntimeLimits::default(),
        )
        .await
        .unwrap();
        let mut snapshot = instance.snapshot.clone();
        snapshot.machine_id = id::<MachineId>("another-machine");

        let result = MachineInstance::restore(
            definition,
            snapshot,
            Box::new(MockExecutor::default()),
            RuntimeLimits::default(),
        );
        assert!(matches!(
            result,
            Err(DispatchError::SnapshotMachineMismatch { expected, actual })
                if expected == id::<MachineId>("test-machine")
                    && actual == id::<MachineId>("another-machine")
        ));
    }

    #[tokio::test]
    async fn restore_rejects_oversized_lifecycle_payloads() {
        let definition = machine(Vec::new(), vec![state("a", None, None, None, Vec::new())]);
        let instance = MachineInstance::start(
            Arc::clone(&definition),
            Box::new(MockExecutor::default()),
            context("ok"),
            RuntimeLimits::default(),
        )
        .await
        .unwrap();
        let mut snapshot = instance.snapshot.clone();
        snapshot.lifecycle = Lifecycle::Cancelled {
            reason: Some(context("too-large")),
        };
        let limits = RuntimeLimits {
            max_payload_bytes: 4,
            ..RuntimeLimits::default()
        };

        let result = MachineInstance::restore(
            definition,
            snapshot,
            Box::new(MockExecutor::default()),
            limits,
        );
        assert!(matches!(result, Err(DispatchError::PayloadTooLarge)));
    }

    #[test]
    fn adapter_contract_has_no_wasmtime_types() {
        fn accepts_adapter<T: crate::DefinitionAdapter>() {}
        fn accepts_factory<T: crate::FunctionExecutorFactory>() {}

        let _ = accepts_adapter::<NeverAdapter>;
        let _ = accepts_factory::<NeverFactory>;
        assert_eq!(ActionKind::wasm_component().as_str(), "wasm-component");
    }

    struct NeverAdapter;

    #[async_trait]
    impl crate::DefinitionAdapter for NeverAdapter {
        async fn load_definition(
            &self,
            _: crate::ArtifactBytes,
            _: &LoadLimits,
        ) -> Result<MachineDefinition, crate::AdapterError> {
            unreachable!()
        }
    }

    struct NeverFactory;

    #[async_trait]
    impl crate::FunctionExecutorFactory for NeverFactory {
        async fn create(&self) -> Result<Box<dyn FunctionExecutor>, RuntimeFault> {
            unreachable!()
        }
    }
}
