use shiroha_core::error::{Result, ShirohaError};
use shiroha_core::flow::{FlowManifest, StateDef, StateKind, TransitionDef};

pub struct TransitionResult {
    pub from: String,
    pub to: String,
    pub action: Option<String>,
    pub guard: Option<String>,
}

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

    pub fn is_terminal(&self, state: &str) -> bool {
        self.get_state(state)
            .is_some_and(|s| s.kind == StateKind::Terminal)
    }
}
