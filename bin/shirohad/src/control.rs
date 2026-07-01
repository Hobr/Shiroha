//! Unix socket control server.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use shiroha_control::{Request, Response};
use shiroha_engine::{Event, TaskManager};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

/// Run the Unix socket server.
pub async fn run_socket_server(
    task_manager: TaskManager,
    component_paths: Arc<RwLock<HashMap<String, PathBuf>>>,
    socket_path: PathBuf,
    cancel_token: CancellationToken,
) -> Result<()> {
    // Clean up old socket file
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("Failed to bind socket: {}", socket_path.display()))?;

    info!("Unix socket listening: {}", socket_path.display());

    loop {
        tokio::select! {
            biased;
            () = cancel_token.cancelled() => {
                break;
            }
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _)) => {
                        let tm = task_manager.clone();
                        let cp = component_paths.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, tm, cp).await {
                                error!("Connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Accept error: {}", e);
                    }
                }
            }
        }
    }

    // Clean up socket file
    let _ = std::fs::remove_file(&socket_path);
    info!("Socket server stopped");
    Ok(())
}

/// Handle a single client connection.
async fn handle_connection(
    stream: UnixStream,
    task_manager: TaskManager,
    component_paths: Arc<RwLock<HashMap<String, PathBuf>>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let resp = match serde_json::from_str::<Request>(&line) {
            Ok(req) => dispatch(req, &task_manager, &component_paths).await,
            Err(e) => Response::error(format!("Invalid request: {}", e)),
        };

        let resp_line = serde_json::to_string(&resp)?;
        writer.write_all(resp_line.as_bytes()).await?;
        writer.write_all(b"\n").await?;
    }

    Ok(())
}

/// Dispatch a request to the appropriate handler.
async fn dispatch(
    req: Request,
    task_manager: &TaskManager,
    component_paths: &Arc<RwLock<HashMap<String, PathBuf>>>,
) -> Response {
    match req {
        Request::ListTasks => {
            let tasks = task_manager.list_tasks().await;
            Response::ok(serde_json::json!({"tasks": tasks}))
        }
        Request::SendEvent { task_id, event } => {
            match task_manager.get_task(&task_id).await {
                Some(handle) => match handle.send(Event {
                    name: event,
                    payload: None,
                }) {
                    Ok(()) => {
                        let state = handle.get_state().await;
                        Response::ok(serde_json::json!({"new_state": state.current_state}))
                    }
                    Err(e) => Response::error(format!("Failed to send event: {}", e)),
                },
                None => Response::error(format!("Task not found: {}", task_id)),
            }
        }
        Request::TaskStatus { task_id } => match task_manager.get_task(&task_id).await {
            Some(handle) => {
                let state = handle.get_state().await;
                let component = component_paths
                    .read()
                    .await
                    .get(&task_id)
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                Response::ok(serde_json::json!({
                    "task_id": state.task_id,
                    "current_state": state.current_state,
                    "component": component,
                }))
            }
            None => Response::error(format!("Task not found: {}", task_id)),
        },
    }
}
