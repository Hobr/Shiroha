//! Core IR types for state machine definitions.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::error::{IrError, Result};

/// Unique identifier for a state.
pub type StateId = String;

/// Unique identifier for an event.
pub type EventId = String;

/// The complete state machine definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachineDef {
    pub name: String,
    pub initial: StateId,
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
    pub events: Vec<EventDef>,
}

impl StateMachineDef {
    /// Validate the state machine definition.
    pub fn validate(&self) -> Result<()> {
        // Check for duplicate state IDs
        let mut state_ids = HashSet::new();
        for state in &self.states {
            if !state_ids.insert(&state.id) {
                return Err(IrError::DuplicateStateId(state.id.clone()));
            }
        }

        // Check for duplicate event IDs
        let mut event_ids = HashSet::new();
        for event in &self.events {
            if !event_ids.insert(&event.name) {
                return Err(IrError::DuplicateEventId(event.name.clone()));
            }
        }

        // Build state map for quick lookup
        let state_map: HashMap<&StateId, &State> = self.states.iter().map(|s| (&s.id, s)).collect();

        // Validate initial state exists
        if !state_map.contains_key(&self.initial) {
            return Err(IrError::InitialStateNotFound(self.initial.clone()));
        }

        // Validate parent references and check for circular nesting
        for state in &self.states {
            if let Some(parent_id) = &state.parent {
                if !state_map.contains_key(parent_id) {
                    return Err(IrError::ParentStateNotFound(parent_id.clone()));
                }

                // Check for circular nesting
                self.check_circular_nesting(&state.id, &state_map)?;
            }
        }

        // Build event map
        let event_map: HashSet<&EventId> = self.events.iter().map(|e| &e.name).collect();

        // Validate transitions
        for transition in &self.transitions {
            // Check from state exists
            if !state_map.contains_key(&transition.from) {
                return Err(IrError::TransitionStateNotFound {
                    from: transition.from.clone(),
                    to: transition.to.clone(),
                });
            }

            // Check to state exists
            if !state_map.contains_key(&transition.to) {
                return Err(IrError::TransitionStateNotFound {
                    from: transition.from.clone(),
                    to: transition.to.clone(),
                });
            }

            // Check event exists
            if !event_map.contains(&transition.event) {
                return Err(IrError::TransitionEventNotFound {
                    from: transition.from.clone(),
                    to: transition.to.clone(),
                    event: transition.event.clone(),
                });
            }
        }

        Ok(())
    }

    /// Check for circular nesting starting from a state.
    fn check_circular_nesting(
        &self,
        state_id: &StateId,
        state_map: &HashMap<&StateId, &State>,
    ) -> Result<()> {
        let mut visited = HashSet::new();
        let mut current = state_id;

        while let Some(state) = state_map.get(current) {
            if !visited.insert(current) {
                return Err(IrError::CircularNesting(format!(
                    "State '{}' has circular parent reference",
                    state_id
                )));
            }

            if let Some(parent) = &state.parent {
                current = parent;
            } else {
                break;
            }
        }

        Ok(())
    }
}

/// A state in the state machine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub id: StateId,
    pub parent: Option<StateId>,
    pub entry: Option<ActionRef>,
    pub exit: Option<ActionRef>,
    /// At most one async do-activity per state.
    pub do_activity: Option<ActionRef>,
    pub history: HistoryConfig,
    /// Reserved for orthogonal regions (MVP: unused).
    #[allow(dead_code)]
    pub ortho: Option<OrthogonalRegion>,
}

/// Placeholder for orthogonal regions (future extension).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrthogonalRegion {
    // TODO: Define orthogonal region structure for future use
}

/// A transition between states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub from: StateId,
    pub to: StateId,
    pub event: EventId,
    pub guard: Option<GuardRef>,
    /// Synchronous transition action.
    pub action: Option<ActionRef>,
}

/// Reference to an action (Wasm or Plugin).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRef {
    pub name: String,
    pub kind: ActionKind,
}

/// The kind of action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionKind {
    Wasm(String),
    Plugin(String),
}

/// Reference to a guard condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GuardRef {
    Always,
    Wasm(String),
    Plugin(String),
}

/// History configuration for a state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum HistoryConfig {
    #[default]
    None,
    Shallow,
    Deep,
}

/// Event definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventDef {
    pub name: EventId,
}
