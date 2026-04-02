use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowManifest {
    pub id: String,
    pub states: Vec<StateDef>,
    pub transitions: Vec<TransitionDef>,
    pub initial_state: String,
    pub actions: Vec<ActionDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDef {
    pub name: String,
    pub kind: StateKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_enter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_exit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subprocess: Option<SubprocessDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateKind {
    Normal,
    Terminal,
    Fork,
    Join,
    Subprocess,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubprocessDef {
    pub flow_id: String,
    pub completion_event: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionDef {
    pub from: String,
    pub to: String,
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<TimeoutDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutDef {
    pub duration_ms: u64,
    pub timeout_event: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionDef {
    pub name: String,
    pub dispatch: DispatchMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchMode {
    Local,
    Remote,
    FanOut(FanOutConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanOutConfig {
    pub strategy: FanOutStrategy,
    pub aggregator: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_success: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanOutStrategy {
    All,
    Count(u32),
    Tagged(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowRegistration {
    pub flow_id: String,
    pub version: Uuid,
    pub manifest: FlowManifest,
    pub wasm_hash: String,
}
