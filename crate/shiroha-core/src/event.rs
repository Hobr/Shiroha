use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::job::ExecutionStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub id: Uuid,
    pub job_id: Uuid,
    pub timestamp_ms: u64,
    pub kind: EventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum EventKind {
    Created {
        flow_id: String,
        flow_version: Uuid,
        initial_state: String,
    },
    Transition {
        event: String,
        from: String,
        to: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        action: Option<String>,
    },
    ActionComplete {
        action: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node_id: Option<String>,
        status: ExecutionStatus,
    },
    Paused,
    Resumed,
    Cancelled,
    Completed {
        final_state: String,
    },
}
