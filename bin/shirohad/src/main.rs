//! Shiroha daemon - single-machine state machine runtime.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use shiroha_engine::{ActionInvoker, Adapter, GuardEvaluator, TaskManager};
use shiroha_wasm::{Engine, WasmActionInvoker, WasmAdapter};
use tokio::signal;
use tracing::{Level, error, info};
use tracing_subscriber::FmtSubscriber;
use tracing_subscriber::fmt::format::FmtSpan;

/// Shiroha daemon CLI arguments.
#[derive(Parser, Debug)]
#[command(
    name = "shirohad",
    version,
    about = "Shiroha daemon - state machine runtime"
)]
struct Cli {
    /// Path to WASM component
    #[arg(long)]
    component: PathBuf,

    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: Level,

    /// Log format (pretty, json)
    #[arg(long, default_value = "pretty")]
    log_format: LogFormat,
}

#[derive(Debug, Clone, Copy)]
enum LogFormat {
    Pretty,
    Json,
}

impl std::str::FromStr for LogFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "pretty" => Ok(LogFormat::Pretty),
            "json" => Ok(LogFormat::Json),
            _ => Err(anyhow::anyhow!("Invalid log format: {}", s)),
        }
    }
}

/// Placeholder guard evaluator (always returns true).
struct NoopGuardEvaluator;

#[async_trait::async_trait]
impl GuardEvaluator for NoopGuardEvaluator {
    async fn evaluate(
        &self,
        _guard: &str,
        _ctx: &shiroha_engine::ActionContext,
    ) -> anyhow::Result<bool> {
        Ok(true)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let args = Cli::parse();

    // Initialize tracing
    init_tracing(&args)?;

    info!("Starting Shiroha daemon");
    info!("Component path: {}", args.component.display());

    // Create WASM engine
    let mut config = wasmtime::Config::new();
    config.wasm_component_model(true);
    let engine = Arc::new(Engine::new(&config)?);

    // Load WASM component
    info!("Loading component...");
    let adapter = WasmAdapter::from_file(engine.clone(), &args.component)
        .context("Failed to create WASM adapter")?;

    let def = adapter
        .load()
        .await
        .context("Failed to load state machine definition")?;

    info!(
        "Component loaded: {} states, {} transitions, {} events",
        def.states.len(),
        def.transitions.len(),
        def.events.len()
    );
    info!("Initial state: {}", def.initial);

    // Create action invoker
    let action_invoker: Arc<dyn ActionInvoker> = Arc::new(WasmActionInvoker::from_file(
        engine.clone(),
        &args.component,
    )?);

    // Create guard evaluator
    let guard_evaluator: Arc<dyn GuardEvaluator> = Arc::new(NoopGuardEvaluator);

    // Create task manager
    let task_manager = TaskManager::new();

    // Create task with fixed ID "default"
    let task_id = "default".to_string();
    info!("Creating task: id={}", task_id);

    let _handle = task_manager
        .create_task(task_id.clone(), def, action_invoker, guard_evaluator)
        .await
        .context("Failed to create task")?;

    info!("Task created successfully");
    info!("Daemon running, press Ctrl-C to stop");

    // Wait for shutdown signal
    match signal::ctrl_c().await {
        Ok(()) => {
            info!("Received shutdown signal");
        }
        Err(err) => {
            error!("Failed to listen for shutdown signal: {}", err);
        }
    }

    info!("Shutting down...");

    Ok(())
}

/// Initialize tracing based on CLI arguments.
fn init_tracing(args: &Cli) -> Result<()> {
    match args.log_format {
        LogFormat::Pretty => {
            let subscriber = FmtSubscriber::builder()
                .with_max_level(args.log_level)
                .with_span_events(FmtSpan::CLOSE)
                .with_target(true)
                .with_thread_ids(false)
                .with_thread_names(false)
                .with_file(false)
                .with_line_number(false)
                .finish();

            tracing::subscriber::set_global_default(subscriber)
                .context("Failed to set tracing subscriber")?;
        }
        LogFormat::Json => {
            // Note: JSON formatting requires additional features
            // For MVP, fall back to compact formatting
            let subscriber = FmtSubscriber::builder()
                .with_max_level(args.log_level)
                .with_span_events(FmtSpan::CLOSE)
                .compact()
                .finish();

            tracing::subscriber::set_global_default(subscriber)
                .context("Failed to set tracing subscriber")?;
        }
    }

    Ok(())
}
