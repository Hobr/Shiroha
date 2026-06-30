//! Shiroha control tool - CLI for managing state machine tasks.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Shiroha control tool CLI arguments.
#[derive(Parser, Debug)]
#[command(name = "sctl", version, about = "Shiroha control tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a new task from a WASM component
    CreateTask {
        /// Path to WASM component
        #[arg(long)]
        component: PathBuf,
    },
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
    /// Stop a running task
    StopTask {
        /// Task ID
        task_id: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::CreateTask { component: _ } => {
            eprintln!("This command will be implemented in v0.3.5");
        }
        Commands::ListTasks => {
            eprintln!("This command will be implemented in v0.3.5");
        }
        Commands::SendEvent {
            task_id: _,
            event_name: _,
        } => {
            eprintln!("This command will be implemented in v0.3.5");
        }
        Commands::TaskStatus { task_id: _ } => {
            eprintln!("This command will be implemented in v0.3.5");
        }
        Commands::StopTask { task_id: _ } => {
            eprintln!("This command will be implemented in v0.3.5");
        }
    }

    Ok(())
}
