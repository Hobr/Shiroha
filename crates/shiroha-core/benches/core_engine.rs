use std::sync::Arc;

use async_trait::async_trait;
use criterion::{Criterion, criterion_group, criterion_main};
use shiroha_core::{
    ActionOutcome, Event, EventName, FunctionExecutor, FunctionRef, GuardInput, HookEffects,
    HookInput, HostInput, LoadLimits, MachineDefinition, MachineId, MachineInstance,
    PayloadEnvelope, RuntimeFault, RuntimeLimits, StateDefinition, StateId, TransitionDefinition,
    Trigger, ValidatedMachine,
};

struct NoopExecutor;

#[async_trait]
impl FunctionExecutor for NoopExecutor {
    async fn evaluate_guard(
        &mut self,
        _: &FunctionRef,
        _: GuardInput,
        _: &shiroha_core::InvocationLimits,
    ) -> Result<bool, RuntimeFault> {
        Ok(true)
    }

    async fn invoke_callback(
        &mut self,
        _: &FunctionRef,
        _: HookInput,
        _: &shiroha_core::InvocationLimits,
    ) -> Result<HookEffects, RuntimeFault> {
        Ok(HookEffects::default())
    }

    async fn invoke_action(
        &mut self,
        _: &FunctionRef,
        _: HookInput,
        _: &shiroha_core::InvocationLimits,
    ) -> Result<ActionOutcome, RuntimeFault> {
        Ok(ActionOutcome::Succeeded(HookEffects::default()))
    }
}

fn definition() -> Arc<ValidatedMachine> {
    Arc::new(
        ValidatedMachine::new(
            MachineDefinition {
                id: MachineId::new("benchmark").unwrap(),
                initial: StateId::new("active").unwrap(),
                functions: Vec::new(),
                states: vec![StateDefinition {
                    id: StateId::new("active").unwrap(),
                    entry: None,
                    exit: None,
                    terminal: None,
                    transitions: vec![TransitionDefinition {
                        trigger: Trigger::Event(EventName::new("tick").unwrap()),
                        guard: None,
                        action: None,
                        target: StateId::new("active").unwrap(),
                        failure_target: None,
                    }],
                }],
            },
            &LoadLimits::default(),
        )
        .unwrap(),
    )
}

fn benchmarks(criterion: &mut Criterion) {
    let definition = definition();
    criterion.bench_function("host_transition_index_lookup", |bencher| {
        bencher.iter(|| {
            definition.transition_indexes(
                &StateId::new("active").unwrap(),
                &Trigger::Event(EventName::new("tick").unwrap()),
            )
        });
    });

    let runtime = tokio::runtime::Runtime::new().unwrap();
    criterion.bench_function("host_start_and_self_transition", |bencher| {
        bencher.iter(|| {
            runtime.block_on(async {
                let mut machine = MachineInstance::start(
                    Arc::clone(&definition),
                    Box::new(NoopExecutor),
                    PayloadEnvelope::json(b"{}".to_vec()),
                    RuntimeLimits::default(),
                )
                .await
                .unwrap();
                machine
                    .dispatch(HostInput::Event(Event::new(
                        EventName::new("tick").unwrap(),
                        None,
                    )))
                    .await
                    .unwrap()
            })
        });
    });
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
