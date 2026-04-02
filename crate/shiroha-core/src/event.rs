//! 事件溯源记录
//!
//! 每次状态转移记录为不可变事件日志，用于审计追踪、故障恢复和调试分析。
//! 事件写入与状态更新在同一事务内，保证一致性。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::job::ExecutionStatus;

/// 事件溯源记录条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub id: Uuid,
    /// 事件流按 Job 分片；存储层通常据此把同一 Job 的事件聚在一起。
    pub job_id: Uuid,
    /// 事件发生时间，由控制面写入，便于跨存储后端统一排序。
    pub timestamp_ms: u64,
    pub kind: EventKind,
}

/// 事件类型
///
/// 使用 `tag = "type"` 的内部标签序列化，便于 JSON 解析时区分事件类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum EventKind {
    /// Job 创建
    Created {
        flow_id: String,
        flow_version: Uuid,
        initial_state: String,
    },
    /// 状态转移
    Transition {
        event: String,
        from: String,
        to: String,
        // 这里只记录“转移时声明的 action 名”，真正执行结果会落在单独的 ActionComplete 事件里。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        action: Option<String>,
    },
    /// Action 执行完成回报
    ActionComplete {
        action: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node_id: Option<String>,
        status: ExecutionStatus,
    },
    Paused,
    Resumed,
    Cancelled,
    /// Job 正常完成
    Completed {
        final_state: String,
    },
}
