//! Tests for IR validation.

use crate::*;

#[test]
fn test_valid_simple_state_machine() {
    let sm = StateMachineDef {
        name: "test".to_string(),
        initial: "idle".to_string(),
        states: vec![
            State {
                id: "idle".to_string(),
                parent: None,
                entry: None,
                exit: None,
                do_activity: None,
                history: HistoryConfig::None,
                ortho: None,
            },
            State {
                id: "active".to_string(),
                parent: None,
                entry: None,
                exit: None,
                do_activity: None,
                history: HistoryConfig::None,
                ortho: None,
            },
        ],
        transitions: vec![Transition {
            from: "idle".to_string(),
            to: "active".to_string(),
            event: "start".to_string(),
            guard: None,
            action: None,
        }],
        events: vec![EventDef {
            name: "start".to_string(),
        }],
    };

    assert!(sm.validate().is_ok());
}

#[test]
fn test_initial_state_not_found() {
    let sm = StateMachineDef {
        name: "test".to_string(),
        initial: "nonexistent".to_string(),
        states: vec![State {
            id: "idle".to_string(),
            parent: None,
            entry: None,
            exit: None,
            do_activity: None,
            history: HistoryConfig::None,
            ortho: None,
        }],
        transitions: vec![],
        events: vec![],
    };

    assert!(matches!(
        sm.validate(),
        Err(IrError::InitialStateNotFound(_))
    ));
}

#[test]
fn test_parent_state_not_found() {
    let sm = StateMachineDef {
        name: "test".to_string(),
        initial: "child".to_string(),
        states: vec![State {
            id: "child".to_string(),
            parent: Some("nonexistent".to_string()),
            entry: None,
            exit: None,
            do_activity: None,
            history: HistoryConfig::None,
            ortho: None,
        }],
        transitions: vec![],
        events: vec![],
    };

    assert!(matches!(
        sm.validate(),
        Err(IrError::ParentStateNotFound(_))
    ));
}

#[test]
fn test_circular_nesting() {
    let sm = StateMachineDef {
        name: "test".to_string(),
        initial: "a".to_string(),
        states: vec![
            State {
                id: "a".to_string(),
                parent: Some("b".to_string()),
                entry: None,
                exit: None,
                do_activity: None,
                history: HistoryConfig::None,
                ortho: None,
            },
            State {
                id: "b".to_string(),
                parent: Some("a".to_string()),
                entry: None,
                exit: None,
                do_activity: None,
                history: HistoryConfig::None,
                ortho: None,
            },
        ],
        transitions: vec![],
        events: vec![],
    };

    assert!(matches!(sm.validate(), Err(IrError::CircularNesting(_))));
}

#[test]
fn test_valid_nested_states() {
    let sm = StateMachineDef {
        name: "test".to_string(),
        initial: "parent".to_string(),
        states: vec![
            State {
                id: "parent".to_string(),
                parent: None,
                entry: None,
                exit: None,
                do_activity: None,
                history: HistoryConfig::Shallow,
                ortho: None,
            },
            State {
                id: "child1".to_string(),
                parent: Some("parent".to_string()),
                entry: None,
                exit: None,
                do_activity: None,
                history: HistoryConfig::None,
                ortho: None,
            },
            State {
                id: "child2".to_string(),
                parent: Some("parent".to_string()),
                entry: None,
                exit: None,
                do_activity: None,
                history: HistoryConfig::None,
                ortho: None,
            },
        ],
        transitions: vec![Transition {
            from: "child1".to_string(),
            to: "child2".to_string(),
            event: "switch".to_string(),
            guard: None,
            action: None,
        }],
        events: vec![EventDef {
            name: "switch".to_string(),
        }],
    };

    assert!(sm.validate().is_ok());
}

#[test]
fn test_transition_event_not_found() {
    let sm = StateMachineDef {
        name: "test".to_string(),
        initial: "a".to_string(),
        states: vec![
            State {
                id: "a".to_string(),
                parent: None,
                entry: None,
                exit: None,
                do_activity: None,
                history: HistoryConfig::None,
                ortho: None,
            },
            State {
                id: "b".to_string(),
                parent: None,
                entry: None,
                exit: None,
                do_activity: None,
                history: HistoryConfig::None,
                ortho: None,
            },
        ],
        transitions: vec![Transition {
            from: "a".to_string(),
            to: "b".to_string(),
            event: "nonexistent".to_string(),
            guard: None,
            action: None,
        }],
        events: vec![],
    };

    assert!(matches!(
        sm.validate(),
        Err(IrError::TransitionEventNotFound { .. })
    ));
}

#[test]
fn test_duplicate_state_id() {
    let sm = StateMachineDef {
        name: "test".to_string(),
        initial: "a".to_string(),
        states: vec![
            State {
                id: "a".to_string(),
                parent: None,
                entry: None,
                exit: None,
                do_activity: None,
                history: HistoryConfig::None,
                ortho: None,
            },
            State {
                id: "a".to_string(),
                parent: None,
                entry: None,
                exit: None,
                do_activity: None,
                history: HistoryConfig::None,
                ortho: None,
            },
        ],
        transitions: vec![],
        events: vec![],
    };

    assert!(matches!(sm.validate(), Err(IrError::DuplicateStateId(_))));
}

#[test]
fn test_duplicate_event_id() {
    let sm = StateMachineDef {
        name: "test".to_string(),
        initial: "a".to_string(),
        states: vec![State {
            id: "a".to_string(),
            parent: None,
            entry: None,
            exit: None,
            do_activity: None,
            history: HistoryConfig::None,
            ortho: None,
        }],
        transitions: vec![],
        events: vec![
            EventDef {
                name: "start".to_string(),
            },
            EventDef {
                name: "start".to_string(),
            },
        ],
    };

    assert!(matches!(sm.validate(), Err(IrError::DuplicateEventId(_))));
}
