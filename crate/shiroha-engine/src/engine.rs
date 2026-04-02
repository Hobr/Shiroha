//! 状态机驱动器
//!
//! [`StateMachineEngine`] 持有一份 [`FlowManifest`]，根据当前状态和事件查找匹配的转移。
//! 它本身不管理 Job 状态——只做纯逻辑的"给定状态+事件，返回转移结果"。

use shiroha_core::error::{Result, ShirohaError};
use shiroha_core::flow::{FlowManifest, StateDef, StateKind, TransitionDef};

/// 状态转移结果
pub struct TransitionResult {
    pub from: String,
    pub to: String,
    /// 转移时需要执行的 action（可选）
    pub action: Option<String>,
    /// 转移前需要评估的 guard（可选）
    pub guard: Option<String>,
}

/// 状态机引擎，负责根据事件查找合法转移
///
/// 无状态设计：每个 Flow 对应一个 Engine 实例，可被多个 Job 共享。
pub struct StateMachineEngine {
    manifest: FlowManifest,
}

impl StateMachineEngine {
    pub fn new(manifest: FlowManifest) -> Self {
        Self { manifest }
    }

    pub fn manifest(&self) -> &FlowManifest {
        &self.manifest
    }

    /// 处理事件：在当前状态下查找匹配的转移
    ///
    /// 取第一个匹配的转移（多个匹配时按声明顺序优先）。
    /// 未找到匹配转移时返回 `InvalidTransition` 错误。
    pub fn process_event(&self, current_state: &str, event: &str) -> Result<TransitionResult> {
        let transitions = self.find_transitions(current_state, event);
        let t = transitions
            .first()
            .ok_or_else(|| ShirohaError::InvalidTransition {
                from: current_state.to_string(),
                to: String::new(),
                event: event.to_string(),
            })?;
        Ok(TransitionResult {
            from: t.from.clone(),
            to: t.to.clone(),
            action: t.action.clone(),
            guard: t.guard.clone(),
        })
    }

    /// 查找从指定状态出发、匹配指定事件的所有转移
    pub fn find_transitions<'a>(&'a self, state: &str, event: &str) -> Vec<&'a TransitionDef> {
        self.manifest
            .transitions
            .iter()
            .filter(|t| t.from == state && t.event == event)
            .collect()
    }

    pub fn get_state(&self, name: &str) -> Option<&StateDef> {
        self.manifest.states.iter().find(|s| s.name == name)
    }

    /// 判断给定状态是否为终态
    pub fn is_terminal(&self, state: &str) -> bool {
        self.get_state(state)
            .is_some_and(|s| s.kind == StateKind::Terminal)
    }
}
