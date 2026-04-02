//! 传输层抽象
//!
//! 定义 [`Transport`] trait 作为节点间通信的统一接口。
//! 内置 [`InProcessTransport`] 用于 standalone 模式（Controller 与 Node 同进程）。
//! Phase 2 将添加 gRPC transport 用于分布式部署。

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// 集群中的节点信息，由 Node 心跳上报
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    /// 节点能力标签（如 "gpu", "high-memory"），用于调度决策
    pub tags: Vec<String>,
    /// 负载水平 0.0~1.0
    pub load: f32,
    /// 当前正在执行的任务数
    pub active_tasks: u32,
}

/// 节点间传输消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub payload: Vec<u8>,
}

/// 节点间传输响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub payload: Vec<u8>,
}

/// 传输后端 trait
///
/// 抽象节点间通信，上层调度逻辑不感知具体协议。
pub trait Transport: Send + Sync {
    /// 向指定节点发送消息
    fn send(&self, target: &str, msg: Message) -> impl Future<Output = Result<Response>> + Send;
    /// 广播消息到所有节点
    fn broadcast(&self, msg: Message) -> impl Future<Output = Result<Vec<Response>>> + Send;
}

/// 进程内传输（standalone 模式用）
///
/// 直接回显消息，不经过网络。用于单进程部署和测试。
#[derive(Debug, Default)]
pub struct InProcessTransport;

impl InProcessTransport {
    pub fn new() -> Self {
        Self
    }
}

impl Transport for InProcessTransport {
    async fn send(&self, _target: &str, msg: Message) -> Result<Response> {
        Ok(Response {
            payload: msg.payload,
        })
    }

    async fn broadcast(&self, msg: Message) -> Result<Vec<Response>> {
        Ok(vec![Response {
            payload: msg.payload,
        }])
    }
}
