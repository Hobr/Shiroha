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
                let has_outgoing = manifest.transitions.iter().any(|t| t.from == state.name);
                if has_outgoing {
                    warnings.push(ValidationWarning::TerminalWithOutgoing(state.name.clone()));
                }
            }
        }

        // BFS 可达性分析：从初始状态出发，检测不可达的状态
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
