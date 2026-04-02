//! Flow 静态验证器
//!
//! 在 Flow 部署时对 manifest 进行静态检查，尽早发现配置错误：
//! - 初始状态是否存在
//! - 转移引用的状态是否存在
//! - 不可达状态检测（BFS）
//! - 终态不应有出边
//! - Action/Guard 引用是否在 actions 列表中声明

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

use shiroha_core::flow::FlowManifest;

/// 验证警告
#[derive(Debug)]
pub enum ValidationWarning {
    InvalidInitialState(String),
    MissingState { field: String, state: String },
    UnreachableState(String),
    TerminalWithOutgoing(String),
    MissingAction(String),
    MissingGuard(String),
}

impl fmt::Display for ValidationWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInitialState(s) => write!(f, "initial state `{s}` not found in states"),
            Self::MissingState { field, state } => {
                write!(f, "transition {field} references missing state `{state}`")
            }
            Self::UnreachableState(s) => {
                write!(f, "state `{s}` is unreachable from initial state")
            }
            Self::TerminalWithOutgoing(s) => {
                write!(f, "terminal state `{s}` has outgoing transitions")
            }
            Self::MissingAction(s) => {
                write!(f, "action `{s}` referenced in transitions but not declared")
            }
            Self::MissingGuard(s) => {
                write!(f, "guard `{s}` referenced in transitions but not declared")
            }
        }
    }
}

pub struct FlowValidator;

impl FlowValidator {
    /// 验证 FlowManifest，返回所有发现的警告
    pub fn validate(manifest: &FlowManifest) -> Vec<ValidationWarning> {
        let mut warnings = Vec::new();

        let state_names: HashSet<&str> = manifest.states.iter().map(|s| s.name.as_str()).collect();
        let action_names: HashSet<&str> =
            manifest.actions.iter().map(|a| a.name.as_str()).collect();
        // 当前 manifest 没有单独的 guard 注册表，因此 guard/action 都从 actions 列表校验。

        // 初始状态必须存在
        if !state_names.contains(manifest.initial_state.as_str()) {
            warnings.push(ValidationWarning::InvalidInitialState(
                manifest.initial_state.clone(),
            ));
        }

        // 检查转移引用的状态和函数
        for t in &manifest.transitions {
            if !state_names.contains(t.from.as_str()) {
                warnings.push(ValidationWarning::MissingState {
                    field: "from".into(),
                    state: t.from.clone(),
                });
            }
            if !state_names.contains(t.to.as_str()) {
                warnings.push(ValidationWarning::MissingState {
                    field: "to".into(),
                    state: t.to.clone(),
                });
            }
            if let Some(ref action) = t.action
                && !action_names.contains(action.as_str())
            {
                warnings.push(ValidationWarning::MissingAction(action.clone()));
            }
            if let Some(ref guard) = t.guard
                && !action_names.contains(guard.as_str())
            {
                warnings.push(ValidationWarning::MissingGuard(guard.clone()));
            }
        }

        // 终态不应有出边
        for state in &manifest.states {
            if state.kind == shiroha_core::flow::StateKind::Terminal {
                // 这里给 warning 而不是 error，让部署流程可以决定是否容忍这类“可疑但不致命”的拓扑。
                let has_outgoing = manifest.transitions.iter().any(|t| t.from == state.name);
                if has_outgoing {
                    warnings.push(ValidationWarning::TerminalWithOutgoing(state.name.clone()));
                }
            }
        }

        // BFS 可达性分析：只看静态拓扑，不考虑 guard 是否可能在运行时拒绝某条边。
        let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
        for t in &manifest.transitions {
            adj.entry(t.from.as_str()).or_default().push(t.to.as_str());
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        if state_names.contains(manifest.initial_state.as_str()) {
            queue.push_back(manifest.initial_state.as_str());
            visited.insert(manifest.initial_state.as_str());
        }
        while let Some(current) = queue.pop_front() {
            if let Some(neighbors) = adj.get(current) {
                for &next in neighbors {
                    if visited.insert(next) {
                        queue.push_back(next);
                    }
                }
            }
        }

        for state in &manifest.states {
            if !visited.contains(state.name.as_str()) {
                warnings.push(ValidationWarning::UnreachableState(state.name.clone()));
            }
        }

        warnings
    }
}

#[cfg(test)]
mod tests {
    use shiroha_core::flow::{ActionDef, DispatchMode, StateDef, StateKind, TransitionDef};

    use super::*;

    fn valid_manifest() -> FlowManifest {
        FlowManifest {
            id: "valid".into(),
            states: vec![
                StateDef {
                    name: "idle".into(),
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
            transitions: vec![TransitionDef {
                from: "idle".into(),
                to: "done".into(),
                event: "finish".into(),
                guard: Some("allow".into()),
                action: Some("ship".into()),
                timeout: None,
            }],
            initial_state: "idle".into(),
            actions: vec![
                ActionDef {
                    name: "ship".into(),
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
    fn valid_manifest_produces_no_warnings() {
        let warnings = FlowValidator::validate(&valid_manifest());

        assert!(warnings.is_empty());
    }

    #[test]
    fn invalid_manifest_reports_expected_warnings() {
        let mut manifest = valid_manifest();
        manifest.initial_state = "ghost".into();
        manifest.states.push(StateDef {
            name: "orphan".into(),
            kind: StateKind::Normal,
            on_enter: None,
            on_exit: None,
            subprocess: None,
        });
        manifest.actions.clear();
        manifest.transitions = vec![
            TransitionDef {
                from: "idle".into(),
                to: "missing".into(),
                event: "finish".into(),
                guard: Some("allow".into()),
                action: Some("ship".into()),
                timeout: None,
            },
            TransitionDef {
                from: "done".into(),
                to: "idle".into(),
                event: "rewind".into(),
                guard: None,
                action: None,
                timeout: None,
            },
        ];

        let warnings = FlowValidator::validate(&manifest);

        assert!(warnings.iter().any(
            |warning| matches!(warning, ValidationWarning::InvalidInitialState(state) if state == "ghost")
        ));
        assert!(warnings.iter().any(|warning| matches!(
            warning,
            ValidationWarning::MissingState { field, state } if field == "to" && state == "missing"
        )));
        assert!(warnings.iter().any(
            |warning| matches!(warning, ValidationWarning::MissingAction(name) if name == "ship")
        ));
        assert!(warnings.iter().any(
            |warning| matches!(warning, ValidationWarning::MissingGuard(name) if name == "allow")
        ));
        assert!(warnings.iter().any(|warning| matches!(
            warning,
            ValidationWarning::TerminalWithOutgoing(state) if state == "done"
        )));
        assert!(warnings.iter().any(
            |warning| matches!(warning, ValidationWarning::UnreachableState(state) if state == "orphan")
        ));
    }
}
