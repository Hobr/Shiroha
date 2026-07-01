//! REPL (Read-Eval-Print Loop) for interactive control.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use shiroha_engine::{Event, TaskManager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Run the interactive REPL.
pub async fn run_repl(
    task_manager: TaskManager,
    component_paths: Arc<RwLock<HashMap<String, PathBuf>>>,
    cancel_token: CancellationToken,
) -> Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    println!("Shiroha REPL. Type 'help' for commands.");

    loop {
        print!("> ");
        std::io::stdout().flush()?;

        tokio::select! {
            biased;
            () = cancel_token.cancelled() => break,

            line = reader.next_line() => {
                match line? {
                    Some(input) => {
                        if let Some(true) = handle_repl_command(&input, &task_manager, &component_paths).await {
                            cancel_token.cancel();
                            break;
                        }
                    }
                    None => break, // EOF
                }
            }
        }
    }

    Ok(())
}

/// Handle a single REPL command. Returns Some(true) if should quit.
async fn handle_repl_command(
    input: &str,
    task_manager: &TaskManager,
    component_paths: &Arc<RwLock<HashMap<String, PathBuf>>>,
) -> Option<bool> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    match parts.first() {
        Some(&"status") => {
            let ids = task_manager.list_tasks().await;
            if ids.is_empty() {
                println!("No tasks running");
            } else {
                for id in ids {
                    if let Some(handle) = task_manager.get_task(&id).await {
                        let state = handle.get_state().await;
                        let component = component_paths
                            .read()
                            .await
                            .get(&id)
                            .map(|p| p.display().to_string())
                            .unwrap_or_default();
                        println!(
                            "  task: {}, state: {}, component: {}",
                            state.task_id, state.current_state, component
                        );
                    }
                }
            }
        }
        Some(&"list-tasks") => {
            let ids = task_manager.list_tasks().await;
            if ids.is_empty() {
                println!("No tasks running");
            } else {
                println!("{}", ids.join(", "));
            }
        }
        Some(&"send-event") if parts.len() == 3 => {
            let task_id = parts[1].to_string();
            match task_manager.get_task(&task_id).await {
                Some(handle) => match handle.send(Event {
                    name: parts[2].to_string(),
                    payload: None,
                }) {
                    Ok(()) => {
                        let state = handle.get_state().await;
                        println!("Transition: -> {}", state.current_state);
                    }
                    Err(e) => println!("Error: {}", e),
                },
                None => println!("Task not found: {}", parts[1]),
            }
        }
        Some(&"send-event") => {
            println!("Usage: send-event <task-id> <event-name>");
        }
        Some(&"help") => print_help(),
        Some(&"quit") | Some(&"exit") => return Some(true),
        Some(cmd) => println!("Unknown command: {}. Type 'help' for commands.", cmd),
        None => {}
    }
    Some(false)
}

/// Print help message.
fn print_help() {
    println!("Available commands:");
    println!("  status                    - Show all task states");
    println!("  list-tasks                - List all task IDs");
    println!("  send-event <id> <event>   - Send event to task");
    println!("  help                      - Show this help");
    println!("  quit / exit               - Exit REPL");
}
