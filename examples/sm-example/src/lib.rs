//! Example state machine component implementing the shiroha:sm WIT interface.
//!
//! This component demonstrates a simple state machine with three states:
//! - Idle: Initial state
//! - Processing: Active state
//! - Done: Terminal state
//!
//! Transitions:
//! - Idle --[start]--> Processing (action: log-message)
//! - Processing --[finish]--> Done (action: log-message)

// Allow unsafe_op_in_unsafe_fn for wit-bindgen generated code (Rust 2024 edition requirement)
#![allow(unsafe_op_in_unsafe_fn)]

// Use wit_bindgen macro to generate bindings inline
wit_bindgen::generate!({
    world: "state-machine",
    path: "../../wit/state-machine.wit",
});

use exports::shiroha::sm::actions::{ActionContext, ActionResult, Guest as ActionsGuest};
use exports::shiroha::sm::definition::{EventDef, Guest as DefinitionGuest, State, Transition};
use shiroha::sm::host::{LogLevel, log};
use shiroha::sm::types::{ActionKind, ActionRef, GuardKind, HistoryKind};

struct Component;

impl DefinitionGuest for Component {
    fn initial() -> String {
        "Idle".to_string()
    }

    fn states() -> Vec<State> {
        vec![
            State {
                name: "Idle".to_string(),
                parent: None,
                entry: None,
                exit: None,
                do_activity: None,
                history: HistoryKind::None,
            },
            State {
                name: "Processing".to_string(),
                parent: None,
                entry: Some(ActionRef {
                    name: "log-start".to_string(),
                    kind: ActionKind::Wasm("log-start".to_string()),
                }),
                exit: None,
                do_activity: None,
                history: HistoryKind::None,
            },
            State {
                name: "Done".to_string(),
                parent: None,
                entry: Some(ActionRef {
                    name: "log-finish".to_string(),
                    kind: ActionKind::Wasm("log-finish".to_string()),
                }),
                exit: None,
                do_activity: None,
                history: HistoryKind::None,
            },
        ]
    }

    fn transitions() -> Vec<Transition> {
        vec![
            Transition {
                from: "Idle".to_string(),
                to: "Processing".to_string(),
                event: "start".to_string(),
                guard: Some(GuardKind::Always),
                action: None, // Entry action will log
            },
            Transition {
                from: "Processing".to_string(),
                to: "Done".to_string(),
                event: "finish".to_string(),
                guard: Some(GuardKind::Always),
                action: None, // Entry action will log
            },
        ]
    }

    fn events() -> Vec<EventDef> {
        vec![
            EventDef {
                name: "start".to_string(),
            },
            EventDef {
                name: "finish".to_string(),
            },
        ]
    }
}

impl ActionsGuest for Component {
    fn invoke(ctx: ActionContext) -> Result<ActionResult, String> {
        // The task_id contains the action name passed by the invoker.
        // For entry/exit actions, the event field contains "entry"/"exit".
        // We use task_id as the action identifier.
        let action_name = ctx.task_id.as_str();

        match action_name {
            "log-start" => {
                log(LogLevel::Info, "Started processing");
                Ok(ActionResult::Ok)
            }
            "log-finish" => {
                log(LogLevel::Info, "Finished processing");
                Ok(ActionResult::Ok)
            }
            _ => {
                // For this simple example, any other action is a no-op
                log(LogLevel::Debug, &format!("Unknown action: {}", action_name));
                Ok(ActionResult::Ok)
            }
        }
    }

    fn invoke_do(ctx: ActionContext) -> Result<ActionResult, String> {
        // For this simple example, do-activities use the same implementation
        Self::invoke(ctx)
    }
}

export!(Component);
