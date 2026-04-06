//! Job 运行实例与执行结果类型
//!
//! Job 是 Flow 的运行实例，绑定特定版本的 WASM 模块。
//! 此模块还定义了 Action 执行结果、Node 结果和聚合决策。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Job 生命周期状态
///
/// 状态转移：Running ↔ Paused, Running/Paused → Cancelled, Running → Completed
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    /// 正常运行，响应事件和定时器
    Running,
    /// 暂停，事件入队但不处理，定时器暂停
    Paused,
    /// 强制终止
    Cancelled,
    /// 到达终态，正常结束
    Completed,
}

impl std::fmt::Display for JobState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Paused => write!(f, "paused"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Completed => write!(f, "completed"),
        }
    }
}

/// 暂停期间暂存的事件
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingJobEvent {
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Vec<u8>>,
}

/// 当前状态上已注册的 timeout 快照
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScheduledTimeout {
    pub event: String,
    pub remaining_ms: u64,
}

/// Job 运行实例
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    /// Job 主键，同时也是事件流、定时器和外部 API 的关联键。
    pub id: Uuid,
    pub flow_id: String,
    /// 创建时绑定的 Flow 版本，新版 Flow 部署后旧 Job 继续用旧版
    pub flow_version: Uuid,
    pub state: JobState,
    /// 当前所处的状态机节点名
    pub current_state: String,
    /// 用户自定义上下文数据，框架只透传字节，不解释内容格式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<Vec<u8>>,
    /// 暂停期间收到但尚未处理的事件；需要跟随 Job 快照一起持久化。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_events: Vec<PendingJobEvent>,
    /// 当前状态的 timeout 计划；running 时表示从 `timeout_anchor_ms` 开始倒计时，paused 时表示冻结的剩余时间。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scheduled_timeouts: Vec<ScheduledTimeout>,
    /// timeout 倒计时起点（毫秒时间戳）；paused 或没有 timeout 时为 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_anchor_ms: Option<u64>,
    /// 可选的 Job 最大生存时长（毫秒）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_lifetime_ms: Option<u64>,
    /// wall-clock 生命周期截止时间（毫秒时间戳）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifetime_deadline_ms: Option<u64>,
}

/// Action 执行状态
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Success,
    Failed,
    Timeout,
}

/// 单次 Action 执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    pub status: ExecutionStatus,
    /// guest action 的原始输出，由上层决定是否解释或持久化。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Vec<u8>>,
}

/// 单个 Node 的执行结果（用于 fan-out 聚合）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeResult {
    pub node_id: String,
    pub status: ExecutionStatus,
    /// 保留每个节点返回的原始负载，供聚合函数自行解释。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<Vec<u8>>,
}

/// 聚合决策：fan-out 多 Node 结果聚合后，决定触发哪个事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateDecision {
    /// 聚合后要触发的事件名，驱动状态机转移
    pub event: String,
    /// 可选的上下文补丁，具体如何合并由上层调度/控制逻辑决定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_patch: Option<Vec<u8>>,
}
