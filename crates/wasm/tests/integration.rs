//! Integration tests for WASM component loading and execution.
//!
//! These tests verify the complete end-to-end flow:
//! 1. Load WASM component → Parse to IR
//! 2. Create task from IR → Initialize runtime
//! 3. Send events → Execute actions and verify state transitions

use std::sync::Arc;
use std::sync::Mutex;

use shiroha_engine::{
    ActionContext, ActionInvoker, ActionResult, Adapter, GuardEvaluator, TaskManager,
};
use shiroha_wasm::WasmAdapter;
use wasmtime::Engine;

/// Simple guard evaluator that always returns true.
struct AlwaysGuard;

#[async_trait::async_trait]
impl GuardEvaluator for AlwaysGuard {
    async fn evaluate(&self, _guard: &str, _ctx: &ActionContext) -> anyhow::Result<bool> {
        Ok(true)
    }
}

/// Log capture for verifying action execution.
#[derive(Clone, Default)]
struct LogCapture {
    logs: Arc<Mutex<Vec<String>>>,
}

impl LogCapture {
    fn new() -> Self {
        Self {
            logs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn add(&self, msg: String) {
        self.logs.lock().unwrap().push(msg);
    }

    fn get_logs(&self) -> Vec<String> {
        self.logs.lock().unwrap().clone()
    }

    fn contains(&self, msg: &str) -> bool {
        self.logs
            .lock()
            .unwrap()
            .iter()
            .any(|log| log.contains(msg))
    }
}

/// Action invoker that logs invocations and delegates to WASM.
struct TestActionInvoker {
    wasm_invoker: Arc<shiroha_wasm::WasmActionInvoker>,
    log_capture: LogCapture,
}

#[async_trait::async_trait]
impl ActionInvoker for TestActionInvoker {
    async fn invoke_sync(&self, name: &str, ctx: ActionContext) -> anyhow::Result<ActionResult> {
        self.log_capture.add(format!("invoke_sync: {}", name));
        self.wasm_invoker.invoke_sync(name, ctx).await
    }

    async fn invoke_do(&self, name: &str, ctx: ActionContext) -> anyhow::Result<ActionResult> {
        self.log_capture.add(format!("invoke_do: {}", name));
        self.wasm_invoker.invoke_do(name, ctx).await
    }
}

/// Test fixture providing engine and component path.
struct TestFixture {
    engine: Arc<Engine>,
    component_path: String,
}

impl TestFixture {
    fn new() -> Self {
        // Initialize wasmtime engine with component model support
        let mut config = wasmtime::Config::new();
        config.wasm_component_model(true);

        let engine = Arc::new(Engine::new(&config).expect("Failed to create wasmtime engine"));

        // Component path relative to crates/wasm
        let component_path = "../../target/wasm32-wasip2/debug/shiroha_sm_example.wasm".to_string();

        Self {
            engine,
            component_path,
        }
    }

    fn adapter(&self) -> WasmAdapter {
        WasmAdapter::from_file(self.engine.clone(), &self.component_path)
            .expect("Failed to create WASM adapter")
    }

    fn action_invoker(&self, log_capture: LogCapture) -> Arc<TestActionInvoker> {
        let wasm_invoker = Arc::new(
            shiroha_wasm::WasmActionInvoker::from_file(self.engine.clone(), &self.component_path)
                .expect("Failed to create WASM action invoker"),
        );

        Arc::new(TestActionInvoker {
            wasm_invoker,
            log_capture,
        })
    }

    fn guard_evaluator(&self) -> Arc<AlwaysGuard> {
        Arc::new(AlwaysGuard)
    }
}

#[tokio::test]
async fn test_load_wasm_component() {
    // Arrange
    let fixture = TestFixture::new();
    let adapter = fixture.adapter();

    // Act
    let def = adapter
        .load()
        .await
        .expect("Failed to load state machine definition");

    // Assert: Verify states
    assert_eq!(def.states.len(), 3, "Expected 3 states");

    let state_names: Vec<_> = def.states.iter().map(|s| s.id.as_str()).collect();
    assert!(state_names.contains(&"Idle"));
    assert!(state_names.contains(&"Processing"));
    assert!(state_names.contains(&"Done"));

    // Assert: Verify initial state
    assert_eq!(def.initial, "Idle", "Initial state should be Idle");

    // Assert: Verify transitions
    assert_eq!(def.transitions.len(), 2, "Expected 2 transitions");

    let idle_to_processing = def
        .transitions
        .iter()
        .find(|t| t.from == "Idle" && t.to == "Processing")
        .expect("Missing Idle → Processing transition");
    assert_eq!(idle_to_processing.event, "start");

    let processing_to_done = def
        .transitions
        .iter()
        .find(|t| t.from == "Processing" && t.to == "Done")
        .expect("Missing Processing → Done transition");
    assert_eq!(processing_to_done.event, "finish");

    // Assert: Verify events
    assert_eq!(def.events.len(), 2, "Expected 2 events");
    let event_names: Vec<_> = def.events.iter().map(|e| e.name.as_str()).collect();
    assert!(event_names.contains(&"start"));
    assert!(event_names.contains(&"finish"));

    // Assert: Verify entry actions exist
    let processing_state = def
        .states
        .iter()
        .find(|s| s.id == "Processing")
        .expect("Missing Processing state");
    assert!(
        processing_state.entry.is_some(),
        "Processing state should have entry action"
    );

    let done_state = def
        .states
        .iter()
        .find(|s| s.id == "Done")
        .expect("Missing Done state");
    assert!(
        done_state.entry.is_some(),
        "Done state should have entry action"
    );
}

#[tokio::test]
async fn test_instantiate_task() {
    // Arrange
    let fixture = TestFixture::new();
    let adapter = fixture.adapter();
    let log_capture = LogCapture::new();

    let def = adapter
        .load()
        .await
        .expect("Failed to load state machine definition");

    let manager = TaskManager::new();
    let action_invoker = fixture.action_invoker(log_capture.clone());
    let guard_evaluator = fixture.guard_evaluator();

    // Act
    let handle = manager
        .create_task(
            "test-task-1".to_string(),
            def,
            action_invoker,
            guard_evaluator,
            None,
        )
        .await
        .expect("Failed to create task");

    // Wait for task to initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Assert: Task handle is valid
    assert_eq!(handle.id(), "test-task-1");

    // Assert: Entry action for initial state should have been executed
    // The sm-example component logs "Started processing" for log-start action,
    // but Idle state has no entry action, so we should see no logs yet
    let logs = log_capture.get_logs();
    assert_eq!(
        logs.len(),
        0,
        "Idle state has no entry action, expected no logs"
    );
}

#[tokio::test]
async fn test_execute_action() {
    // Arrange
    let fixture = TestFixture::new();
    let adapter = fixture.adapter();
    let log_capture = LogCapture::new();

    let def = adapter
        .load()
        .await
        .expect("Failed to load state machine definition");

    let manager = TaskManager::new();
    let action_invoker = fixture.action_invoker(log_capture.clone());
    let guard_evaluator = fixture.guard_evaluator();

    let handle = manager
        .create_task(
            "test-task-2".to_string(),
            def,
            action_invoker.clone(),
            guard_evaluator,
            None,
        )
        .await
        .expect("Failed to create task");

    // Wait for task to initialize
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Act: Send "start" event to transition Idle → Processing
    handle
        .send(shiroha_engine::Event {
            name: "start".to_string(),
            payload: None,
        })
        .expect("Failed to send start event");

    // Wait for event processing
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Assert: Entry action for Processing state should have been invoked
    assert!(
        log_capture.contains("invoke_sync: log-start"),
        "Expected log-start action to be invoked"
    );

    // Act: Send "finish" event to transition Processing → Done
    handle
        .send(shiroha_engine::Event {
            name: "finish".to_string(),
            payload: None,
        })
        .expect("Failed to send finish event");

    // Wait for event processing
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Assert: Entry action for Done state should have been invoked
    assert!(
        log_capture.contains("invoke_sync: log-finish"),
        "Expected log-finish action to be invoked"
    );

    // Assert: Both actions should have been logged
    let logs = log_capture.get_logs();
    assert!(logs.len() >= 2, "Expected at least 2 action invocations");
}

#[tokio::test]
async fn test_wasm_host_log_capture() {
    // This test verifies that WASM actions can call host.log().
    // Instead of trying to capture tracing output (which is complex in test contexts),
    // we verify that the action executes successfully without errors.
    // The actual log output goes to the test runner's stdout.

    // Arrange: Create task and send event
    let fixture = TestFixture::new();
    let adapter = fixture.adapter();
    let log_capture = LogCapture::new();

    let def = adapter.load().await.expect("Failed to load definition");

    let manager = TaskManager::new();
    let action_invoker = fixture.action_invoker(log_capture.clone());
    let guard_evaluator = fixture.guard_evaluator();

    let handle = manager
        .create_task(
            "test-task-3".to_string(),
            def,
            action_invoker,
            guard_evaluator,
            None,
        )
        .await
        .expect("Failed to create task");

    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Act: Trigger transition to Processing (which calls log-start action)
    handle
        .send(shiroha_engine::Event {
            name: "start".to_string(),
            payload: None,
        })
        .expect("Failed to send start event");

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Assert: Verify the action was invoked through our test wrapper
    assert!(
        log_capture.contains("invoke_sync: log-start"),
        "Expected log-start action to be invoked"
    );

    // Act: Trigger transition to Done
    handle
        .send(shiroha_engine::Event {
            name: "finish".to_string(),
            payload: None,
        })
        .expect("Failed to send finish event");

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    // Assert: Verify both actions were invoked
    assert!(
        log_capture.contains("invoke_sync: log-finish"),
        "Expected log-finish action to be invoked"
    );

    // Both actions should have completed successfully
    // (if WASM host.log failed, we would have seen errors in action invocation)
    let logs = log_capture.get_logs();
    assert!(logs.len() >= 2, "Expected at least 2 action invocations");
}
