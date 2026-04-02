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

#[cfg(test)]
mod tests {
    use shiroha_core::error::ShirohaError;
    use shiroha_core::flow::{ActionDef, DispatchMode};

    use super::*;

    fn sample_manifest() -> FlowManifest {
        FlowManifest {
            id: "demo".into(),
            states: vec![
                StateDef {
                    name: "idle".into(),
                    kind: StateKind::Normal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: "working".into(),
                    kind: StateKind::Normal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: "done".into(),
                    kind: StateKind::Terminal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
            ],
            transitions: vec![
                TransitionDef {
                    from: "idle".into(),
                    to: "working".into(),
                    event: "start".into(),
                    guard: Some("allow".into()),
                    action: Some("ship".into()),
                    timeout: None,
                },
                TransitionDef {
                    from: "idle".into(),
                    to: "done".into(),
                    event: "start".into(),
                    guard: None,
                    action: Some("fallback".into()),
                    timeout: None,
                },
                TransitionDef {
                    from: "working".into(),
                    to: "done".into(),
                    event: "finish".into(),
                    guard: None,
                    action: None,
                    timeout: None,
                },
            ],
            initial_state: "idle".into(),
            actions: vec![
                ActionDef {
                    name: "ship".into(),
                    dispatch: DispatchMode::Local,
                },
                ActionDef {
                    name: "fallback".into(),
                    dispatch: DispatchMode::Local,
                },
                ActionDef {
                    name: "allow".into(),
                    dispatch: DispatchMode::Local,
                },
            ],
        }
    }

    #[test]
    fn process_event_prefers_first_matching_transition() {
        let engine = StateMachineEngine::new(sample_manifest());

        let result = engine.process_event("idle", "start").expect("transition");

        assert_eq!(result.from, "idle");
        assert_eq!(result.to, "working");
        assert_eq!(result.action.as_deref(), Some("ship"));
        assert_eq!(result.guard.as_deref(), Some("allow"));
    }

    #[test]
    fn process_event_returns_invalid_transition_for_unknown_event() {
        let engine = StateMachineEngine::new(sample_manifest());

        let error = match engine.process_event("idle", "missing") {
            Ok(_) => panic!("should reject unknown event"),
            Err(error) => error,
        };

        match error {
            ShirohaError::InvalidTransition { from, to, event } => {
                assert_eq!(from, "idle");
                assert!(to.is_empty());
                assert_eq!(event, "missing");
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn terminal_detection_follows_state_kind() {
        let engine = StateMachineEngine::new(sample_manifest());

        assert!(!engine.is_terminal("idle"));
        assert!(engine.is_terminal("done"));
        assert!(!engine.is_terminal("missing"));
    }
}
