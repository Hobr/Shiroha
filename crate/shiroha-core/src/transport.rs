//! 传输层抽象
//!
//! 定义 [`Transport`] trait 作为节点间通信的统一接口。
//! 内置 [`InProcessTransport`] 用于 standalone 模式（Controller 与 Node 同进程）。
//! Phase 2 将添加 gRPC transport 用于分布式部署。

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, mpsc, oneshot};

use crate::error::{Result, ShirohaError};

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
    /// Transport 层只搬运不解释 payload，编码格式由上层协议决定。
    pub payload: Vec<u8>,
}

/// 节点间传输响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    /// 与 `Message::payload` 一样保持 opaque bytes，便于替换底层协议。
    pub payload: Vec<u8>,
}

/// In-process transport 暴露给节点执行端的请求封装。
///
/// Transport 层仍然只搬运 opaque bytes；节点执行端自行解释 `message.payload`
/// 和 `respond` 里的 `Response.payload`。
#[derive(Debug)]
pub struct TransportRequest {
    pub message: Message,
    pub respond: oneshot::Sender<Result<Response>>,
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
#[derive(Debug, Default, Clone)]
pub struct InProcessTransport {
    routes: Arc<RwLock<HashMap<String, mpsc::Sender<TransportRequest>>>>,
}

impl InProcessTransport {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个进程内节点，返回该节点用于接收 transport 请求的 channel。
    pub async fn register_node(
        &self,
        node_id: impl Into<String>,
    ) -> mpsc::Receiver<TransportRequest> {
        let (sender, receiver) = mpsc::channel(256);
        self.routes.write().await.insert(node_id.into(), sender);
        receiver
    }

    pub async fn unregister_node(&self, node_id: &str) {
        self.routes.write().await.remove(node_id);
    }
}

impl Transport for InProcessTransport {
    async fn send(&self, target: &str, msg: Message) -> Result<Response> {
        let sender = self
            .routes
            .read()
            .await
            .get(target)
            .cloned()
            .ok_or_else(|| ShirohaError::Transport(format!("node `{target}` not registered")))?;
        let (respond_tx, respond_rx) = oneshot::channel();
        sender
            .send(TransportRequest {
                message: msg,
                respond: respond_tx,
            })
            .await
            .map_err(|_| ShirohaError::Transport(format!("node `{target}` is unavailable")))?;
        respond_rx.await.map_err(|_| {
            ShirohaError::Transport(format!("node `{target}` dropped response channel"))
        })?
    }

    async fn broadcast(&self, msg: Message) -> Result<Vec<Response>> {
        let routes = self.routes.read().await.clone();
        let mut responses = Vec::with_capacity(routes.len());
        for (node_id, sender) in routes {
            let (respond_tx, respond_rx) = oneshot::channel();
            sender
                .send(TransportRequest {
                    message: msg.clone(),
                    respond: respond_tx,
                })
                .await
                .map_err(|_| ShirohaError::Transport(format!("node `{node_id}` is unavailable")))?;
            responses.push(respond_rx.await.map_err(|_| {
                ShirohaError::Transport(format!("node `{node_id}` dropped response channel"))
            })??);
        }
        Ok(responses)
    }
}
