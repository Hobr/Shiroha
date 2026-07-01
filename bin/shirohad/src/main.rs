//! Shiroha daemon - single-machine state machine runtime.

mod daemon;
mod control;
mod repl;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::{Level, info};
use tracing_subscriber::FmtSubscriber;
use tracing_subscriber::fmt::format::FmtSpan;

use daemon::Daemon;
use control::run_socket_server;
use repl::run_repl;
/// Shiroha daemon CLI arguments.
#[derive(Parser, Debug)]
#[command(
    name = "shirohad",
    version,
    about = "Shiroha daemon - state machine runtime"
)]
struct Cli {
    /// Path to WASM component (repeatable)
    #[arg(long)]
    component: Vec<PathBuf>,

    /// Enable interactive REPL mode
    #[arg(long, default_value_t = false)]
    repl: bool,

    /// Unix socket path
    #[arg(long, default_value = "/tmp/shirohad.sock")]
    socket: PathBuf,

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

#[tokio::main]
async fn main() -> Result<()> {
    // Parse CLI arguments
    let args = Cli::parse();

    // Initialize tracing
    init_tracing(&args)?;

    // Validate arguments
    if args.component.is_empty() {
        anyhow::bail!("At least one --component required");
    }

    info!("Starting Shiroha daemon");

    // Create cancellation token
    let cancel_token = CancellationToken::new();

    // Create daemon
    let daemon = Daemon::new(args.socket.clone(), cancel_token.clone());

    // Load components
    let count = daemon.load_components(&args.component).await?;
    info!("{} task(s) created successfully", count);

    // Start Unix socket server
    let socket_handle = tokio::spawn(run_socket_server(
        daemon.task_manager.clone(),
        daemon.component_paths.clone(),
        daemon.socket_path.clone(),
        cancel_token.clone(),
    ));

    // Set up ctrl_c handler
    let ctrl_token = cancel_token.clone();
    tokio::spawn(async move {
        if signal::ctrl_c().await.is_ok() {
            info!("Received shutdown signal");
            ctrl_token.cancel();
        }
    });

    // Run daemon
    if args.repl {
        run_repl(
            daemon.task_manager.clone(),
            daemon.component_paths.clone(),
            cancel_token.clone(),
        )
        .await?;
    } else {
        info!("Daemon running, press Ctrl-C to stop");
        cancel_token.cancelled().await;
    }

    // Wait for socket server to stop
    let _ = socket_handle.await;
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
