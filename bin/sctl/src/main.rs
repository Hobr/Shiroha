//! Shiroha control tool - CLI for managing state machine tasks.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use shiroha_control::{Request, Response, ResponseStatus};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Shiroha control tool CLI arguments.
#[derive(Parser, Debug)]
#[command(name = "sctl", version, about = "Shiroha control tool")]
struct Cli {
    /// Unix socket path
    #[arg(long, default_value = "/tmp/shirohad.sock", global = true)]
    socket: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List all running tasks
    ListTasks,
    /// Send an event to a task
    SendEvent {
        /// Task ID
        task_id: String,
        /// Event name
        event_name: String,
    },
    /// Get task status
    TaskStatus {
        /// Task ID
        task_id: String,
    },
    /// (v0.4.0) Create a new task from a WASM component
    CreateTask {
        /// Path to WASM component
        #[arg(long)]
        component: PathBuf,
    },
    /// (v0.4.0) Stop a running task
    StopTask {
        /// Task ID
        task_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // v0.4.0 commands - not yet implemented
    match cli.command {
        Commands::CreateTask { .. } | Commands::StopTask { .. } => {
            eprintln!("This command will be implemented in v0.4.0");
            return Ok(());
        }
        _ => {}
    }

    // Connect to shirohad
    let mut stream = UnixStream::connect(&cli.socket)
        .await
        .with_context(|| {
            format!(
                "Cannot connect to shirohad (is it running? socket: {})",
                cli.socket.display()
            )
        })?;

    // Build request
    let req = match &cli.command {
        Commands::ListTasks => Request::ListTasks,
        Commands::SendEvent {
            task_id,
            event_name,
        } => Request::SendEvent {
            task_id: task_id.clone(),
            event: event_name.clone(),
        },
        Commands::TaskStatus { task_id } => Request::TaskStatus {
            task_id: task_id.clone(),
        },
        _ => unreachable!(),
    };

    // Send request
    let req_line = serde_json::to_string(&req)?;
    stream.write_all(req_line.as_bytes()).await?;
    stream.write_all(b"\n").await?;

    // Read response
    let (reader, _writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();
    if let Some(resp_line) = lines.next_line().await? {
        let resp: Response = serde_json::from_str(&resp_line)?;
        match resp.status {
            ResponseStatus::Ok => {
                print_ok_response(&cli.command, resp.data);
            }
            ResponseStatus::Error => {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

/// Print successful response based on command type.
fn print_ok_response(command: &Commands, data: Option<serde_json::Value>) {
    let data = match data {
        Some(d) => d,
        None => return,
    };

    match command {
        Commands::ListTasks => {
            if let Some(tasks) = data.get("tasks").and_then(|v| v.as_array()) {
                let task_ids: Vec<String> = tasks
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                if task_ids.is_empty() {
                    println!("No tasks running");
                } else {
                    println!("{}", task_ids.join(", "));
                }
            }
        }
        Commands::SendEvent { .. } => {
            if let Some(new_state) = data.get("new_state").and_then(|v| v.as_str()) {
                println!("Transition successful, new state: {}", new_state);
            }
        }
        Commands::TaskStatus { .. } => {
            let task_id = data
                .get("task_id")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let state = data
                .get("current_state")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let component = data
                .get("component")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            println!("Task: {}, State: {}, Component: {}", task_id, state, component);
        }
        _ => {}
    }
}
