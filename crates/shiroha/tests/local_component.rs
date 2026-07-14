use std::path::PathBuf;

use shiroha::core::{HostInput, PayloadEnvelope};
use shiroha::{Event, EventName, RunOutcome, ShirohaRuntime, StateId};

fn example_component() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/components/wasm32-wasip2/debug/example_machine.wasm");
    std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "failed to read {}: {error}; run `just build-example` first",
            path.display()
        )
    })
}

#[tokio::test]
async fn local_component_runs_through_the_public_facade() {
    let runtime = ShirohaRuntime::builder().build().unwrap();
    let prepared = runtime
        .prepare_component(example_component())
        .await
        .unwrap();
    let mut machine = prepared
        .start(PayloadEnvelope::json(br#"{"phase":"idle"}"#.to_vec()))
        .await
        .unwrap();

    let unhandled = machine
        .dispatch(HostInput::Event(Event::new(
            EventName::new("unknown").unwrap(),
            None,
        )))
        .await
        .unwrap();
    assert_eq!(unhandled.unhandled.len(), 1);

    let report = machine
        .dispatch(HostInput::Event(Event::new(
            EventName::new("begin").unwrap(),
            None,
        )))
        .await
        .unwrap();
    assert_eq!(report.outcome, RunOutcome::Completed);
    assert_eq!(machine.snapshot().state, StateId::new("done").unwrap());
}

#[tokio::test]
async fn local_component_recreates_guest_from_host_snapshot() {
    let runtime = ShirohaRuntime::builder().build().unwrap();
    let prepared = runtime
        .prepare_component(example_component())
        .await
        .unwrap();
    let mut first = prepared
        .start(PayloadEnvelope::json(br#"{"phase":"idle"}"#.to_vec()))
        .await
        .unwrap();
    first
        .dispatch(HostInput::Event(Event::new(
            EventName::new("pause").unwrap(),
            None,
        )))
        .await
        .unwrap();
    assert_eq!(first.snapshot().state, StateId::new("processing").unwrap());

    let mut restored = prepared.restore(first.snapshot().clone()).await.unwrap();
    let report = restored
        .dispatch(HostInput::Event(Event::new(
            EventName::new("finish").unwrap(),
            None,
        )))
        .await
        .unwrap();
    assert_eq!(report.outcome, RunOutcome::Completed);
    assert_eq!(restored.snapshot().state, StateId::new("done").unwrap());
    assert_eq!(restored.snapshot().context.data(), br#"{"phase":"done"}"#);
}
