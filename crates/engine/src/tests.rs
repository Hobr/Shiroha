//! Tests for the state machine engine.

use std::sync::Arc;

use async_trait::async_trait;

use crate::*;
use shiroha_ir::*;

/// Stub action invoker for testing.
struct StubActionInvoker;

#[async_trait]
impl ActionInvoker for StubActionInvoker {
    async fn invoke_sync(&self, _name: &str, _ctx: ActionContext) -> anyhow::Result<ActionResult> {
        Ok(ActionResult::Ok)
    }

    async fn invoke_do(&self, _name: &str, _ctx: ActionContext) -> anyhow::Result<ActionResult> {
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        Ok(ActionResult::Ok)
    }
}

/// Stub guard evaluator for testing.
struct StubGuardEvaluator;

#[async_trait]
impl GuardEvaluator for StubGuardEvaluator {
    async fn evaluate(&self, _guard: &str, _ctx: &ActionContext) -> anyhow::Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn test_simple_transition() {
    let def = StateMachineDef {
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

    assert!(def.validate().is_ok());

    let invoker = Arc::new(StubActionInvoker);
    let guard = Arc::new(StubGuardEvaluator);

    let (task, handle) = Task::new("test-task".to_string(), def, invoker, guard);
    let task_handle = task.run();

    // Give time for initial state entry
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Send event
    handle
        .send(Event {
            name: "start".to_string(),
            payload: None,
        })
        .unwrap();

    // Give time for transition
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    // Cleanup
    task_handle.abort();
}

#[tokio::test]
async fn test_nested_states() {
    let def = StateMachineDef {
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

    assert!(def.validate().is_ok());

    let invoker = Arc::new(StubActionInvoker);
    let guard = Arc::new(StubGuardEvaluator);

    let (task, handle) = Task::new("test-task".to_string(), def, invoker, guard);
    let task_handle = task.run();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    handle
        .send(Event {
            name: "switch".to_string(),
            payload: None,
        })
        .unwrap();

    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

    task_handle.abort();
}

#[tokio::test]
async fn test_task_manager() {
    let def = StateMachineDef {
        name: "test".to_string(),
        initial: "idle".to_string(),
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

    let manager = TaskManager::new();
    let invoker = Arc::new(StubActionInvoker);
    let guard = Arc::new(StubGuardEvaluator);

    let handle = manager
        .create_task("task1".to_string(), def, invoker, guard)
        .await
        .unwrap();

    assert_eq!(handle.id(), "task1");

    let retrieved = manager.get_task(&"task1".to_string()).await;
    assert!(retrieved.is_some());

    let tasks = manager.list_tasks().await;
    assert_eq!(tasks.len(), 1);
}
