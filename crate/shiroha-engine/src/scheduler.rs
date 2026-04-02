//! 调度策略
//!
//! [`Scheduler`] trait 定义调度接口，Controller 根据策略选择 Node 执行任务。
//! 内置 [`RoundRobinScheduler`] 作为默认策略。
//! Phase 3 将支持通过 WASM 插件自定义调度算法。

use std::sync::atomic::{AtomicUsize, Ordering};

use shiroha_core::transport::NodeInfo;

/// 调度策略 trait
pub trait Scheduler: Send + Sync {
    /// 从可用节点列表中选择一个节点执行指定 action
    fn select_node(&self, nodes: &[NodeInfo], action: &str) -> Option<String>;
}

/// 轮询调度：按顺序循环选择节点
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
