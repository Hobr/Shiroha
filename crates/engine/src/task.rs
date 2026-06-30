//! Task actor and handle for state machine instances.

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

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
}

impl TaskHandle {
    /// Create a new task handle.
    pub fn new(id: TaskId, sender: mpsc::UnboundedSender<Event>) -> Self {
        Self { id, sender }
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
}

/// Current state of a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub task_id: TaskId,
    pub current_state: String,
    pub active_do_activity: Option<String>,
}
