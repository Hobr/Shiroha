use std::sync::atomic::{AtomicUsize, Ordering};

use shiroha_core::transport::NodeInfo;

pub trait Scheduler: Send + Sync {
    fn select_node(&self, nodes: &[NodeInfo], action: &str) -> Option<String>;
}

pub struct RoundRobinScheduler {
    counter: AtomicUsize,
}

impl RoundRobinScheduler {
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }
}

impl Default for RoundRobinScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler for RoundRobinScheduler {
    fn select_node(&self, nodes: &[NodeInfo], _action: &str) -> Option<String> {
        if nodes.is_empty() {
            return None;
        }
        let idx = self.counter.fetch_add(1, Ordering::Relaxed) % nodes.len();
        Some(nodes[idx].id.clone())
    }
}
