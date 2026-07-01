//! Task actor and handle for state machine instances.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, mpsc};

/// Unique identifier for a task instance.
pub type TaskId = String;

/// Event sent to a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub name: String,
    pub payload: Option<Vec<u8>>,
}

/// Handle to a running task (clone-able sender).
#[derive(Clone)]
pub struct TaskHandle {
    id: TaskId,
    sender: mpsc::UnboundedSender<Event>,
    state: Arc<RwLock<TaskState>>,
    component_path: Option<PathBuf>,
}

impl TaskHandle {
    /// Create a new task handle.
    pub fn new(
        id: TaskId,
        sender: mpsc::UnboundedSender<Event>,
        state: Arc<RwLock<TaskState>>,
        component_path: Option<PathBuf>,
    ) -> Self {
        Self {
            id,
            sender,
            state,
            component_path,
        }
    }

    /// Get the task ID.
    pub fn id(&self) -> &TaskId {
        &self.id
    }

    /// Send an event to the task.
    pub fn send(&self, event: Event) -> anyhow::Result<()> {
        self.sender
            .send(event)
            .map_err(|_| anyhow::anyhow!("Task mailbox closed"))?;
        Ok(())
    }

    /// Get the current state of the task.
    pub async fn get_state(&self) -> TaskState {
        self.state.read().await.clone()
    }

    /// Try to get the current state without blocking (returns None if locked).
    pub fn try_get_state(&self) -> Option<TaskState> {
        self.state.try_read().ok().map(|s| s.clone())
    }

    /// Get the component path this task was loaded from.
    pub fn component_path(&self) -> Option<&Path> {
        self.component_path.as_deref()
    }
}

/// Current state of a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub task_id: TaskId,
    pub current_state: String,
    pub active_do_activity: Option<String>,
}
