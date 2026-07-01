//! Control protocol types for shirohad/sctl communication.

use serde::{Deserialize, Serialize};

/// Client request sent over Unix socket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", content = "params")]
pub enum Request {
    /// List all task IDs.
    #[serde(rename = "list-tasks")]
    ListTasks,

    /// Send an event to a specific task.
    #[serde(rename = "send-event")]
    SendEvent { task_id: String, event: String },

    /// Query the current state of a task.
    #[serde(rename = "task-status")]
    TaskStatus { task_id: String },
}

/// Server response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub status: ResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Response status.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResponseStatus {
    Ok,
    Error,
}

impl Response {
    /// Create a successful response with data.
    pub fn ok(data: serde_json::Value) -> Self {
        Self {
            status: ResponseStatus::Ok,
            data: Some(data),
            error: None,
        }
    }

    /// Create an error response.
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            status: ResponseStatus::Error,
            data: None,
            error: Some(msg.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = Request::ListTasks;
        let json = serde_json::to_string(&req).unwrap();
        assert_eq!(json, r#"{"command":"list-tasks"}"#);

        let req = Request::SendEvent {
            task_id: "task1".to_string(),
            event: "start".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""command":"send-event""#));
        assert!(json.contains(r#""task_id":"task1""#));
        assert!(json.contains(r#""event":"start""#));
    }

    #[test]
    fn test_request_deserialization() {
        let json = r#"{"command":"list-tasks"}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        assert!(matches!(req, Request::ListTasks));

        let json = r#"{"command":"send-event","params":{"task_id":"task1","event":"start"}}"#;
        let req: Request = serde_json::from_str(json).unwrap();
        if let Request::SendEvent { task_id, event } = req {
            assert_eq!(task_id, "task1");
            assert_eq!(event, "start");
        } else {
            panic!("Expected SendEvent");
        }
    }

    #[test]
    fn test_response_serialization() {
        let resp = Response::ok(serde_json::json!({"tasks": ["task1"]}));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""status":"ok""#));
        assert!(json.contains(r#""tasks":["task1"]"#));

        let resp = Response::error("Task not found");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""status":"error""#));
        assert!(json.contains(r#""error":"Task not found""#));
    }

    #[test]
    fn test_response_deserialization() {
        let json = r#"{"status":"ok","data":{"tasks":["task1"]}}"#;
        let resp: Response = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, ResponseStatus::Ok);
        assert!(resp.data.is_some());

        let json = r#"{"status":"error","error":"Task not found"}"#;
        let resp: Response = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, ResponseStatus::Error);
        assert_eq!(resp.error.as_deref(), Some("Task not found"));
    }
}
