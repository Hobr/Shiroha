use std::path::PathBuf;

use criterion::{Criterion, criterion_group, criterion_main};
use shiroha_adapter_wasm::WasmMachineLoader;
use shiroha_core::{
    ArtifactBytes, Event, EventName, FunctionId, FunctionRef, GuardInput, HookInput, HostInput,
    InvocationLimits, LoadLimits, PayloadEnvelope, StateId,
};

fn artifact() -> ArtifactBytes {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/components/wasm32-wasip2/debug/example_machine.wasm");
    ArtifactBytes::new(std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "failed to read {}: {error}; run `just build-example` first",
            path.display()
        )
    }))
}

fn benchmarks(criterion: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let loader = WasmMachineLoader::new(InvocationLimits::default()).unwrap();
    let artifact = artifact();

    criterion.bench_function("component_compile_prepare", |bencher| {
        bencher.iter(|| {
            runtime
                .block_on(loader.prepare(artifact.clone(), &LoadLimits::default()))
                .unwrap()
        });
    });

    let prepared = runtime
        .block_on(loader.prepare(artifact, &LoadLimits::default()))
        .unwrap();
    criterion.bench_function("instance_pre_instantiate", |bencher| {
        bencher.iter(|| runtime.block_on(prepared.create_executor()).unwrap());
    });

    let mut executor = runtime.block_on(prepared.create_executor()).unwrap();
    let function = FunctionRef::wasm(FunctionId::new("allow").unwrap());
    let input = GuardInput {
        source_state: StateId::new("idle").unwrap(),
        context: PayloadEnvelope::json(br#"{"phase":"idle"}"#.to_vec()),
        input: HostInput::Event(Event::new(EventName::new("begin").unwrap(), None)),
    };
    criterion.bench_function("warm_guard_call", |bencher| {
        bencher.iter(|| {
            runtime
                .block_on(executor.evaluate_guard(
                    &function,
                    input.clone(),
                    &InvocationLimits::default(),
                ))
                .unwrap()
        });
    });

    let hook_input = HookInput {
        source_state: StateId::new("idle").unwrap(),
        target_state: Some(StateId::new("processing").unwrap()),
        context: PayloadEnvelope::json(br#"{"phase":"idle"}"#.to_vec()),
        input: HostInput::Event(Event::new(EventName::new("begin").unwrap(), None)),
    };
    let callback = FunctionRef::wasm(FunctionId::new("enter-idle").unwrap());
    criterion.bench_function("warm_callback_call", |bencher| {
        bencher.iter(|| {
            runtime
                .block_on(executor.invoke_callback(
                    &callback,
                    hook_input.clone(),
                    &InvocationLimits::default(),
                ))
                .unwrap()
        });
    });

    let action = FunctionRef::wasm(FunctionId::new("pause").unwrap());
    criterion.bench_function("warm_action_call", |bencher| {
        bencher.iter(|| {
            runtime
                .block_on(executor.invoke_action(
                    &action,
                    hook_input.clone(),
                    &InvocationLimits::default(),
                ))
                .unwrap()
        });
    });
}

criterion_group!(benches, benchmarks);
criterion_main!(benches);
