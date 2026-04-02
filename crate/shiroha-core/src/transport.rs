use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub tags: Vec<String>,
    pub load: f32,
    pub active_tasks: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub payload: Vec<u8>,
}

pub trait Transport: Send + Sync {
    fn send(&self, target: &str, msg: Message) -> impl Future<Output = Result<Response>> + Send;
    fn broadcast(&self, msg: Message) -> impl Future<Output = Result<Vec<Response>>> + Send;
}

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
