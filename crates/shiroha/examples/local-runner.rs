use std::path::PathBuf;

use anyhow::{Context, Result};
use shiroha::core::{HostInput, PayloadEnvelope};
use shiroha::{Event, EventName, ShirohaRuntime};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let component_path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .context("usage: cargo run -p shiroha --example local-runner -- <component.wasm>")?;
    let component = std::fs::read(&component_path)
        .with_context(|| format!("failed to read {}", component_path.display()))?;

    let runtime = ShirohaRuntime::builder().build()?;
    let prepared = runtime.prepare_component(component).await?;
    println!("imports: {:?}", prepared.metadata().imports);

    let mut machine = prepared
        .start(PayloadEnvelope::json(br#"{"phase":"idle"}"#.to_vec()))
        .await?;
    let unhandled = machine
        .dispatch(HostInput::Event(Event::new(
            EventName::new("unknown")?,
            None,
        )))
        .await?;
    println!("unhandled inputs: {}", unhandled.unhandled.len());

    let report = machine
        .dispatch(HostInput::Event(Event::new(EventName::new("begin")?, None)))
        .await?;
    println!("outcome: {:?}", report.outcome);
    println!("snapshot: {:?}", machine.snapshot());
    Ok(())
}
