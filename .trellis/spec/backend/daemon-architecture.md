# Daemon Architecture

**Purpose**: Document shirohad daemon's multi-component management, control interface, and concurrency model.

---

## Multi-Component Management

### Task ID Generation

Each loaded WASM component creates a unique task with ID format:

```
<component-stem>-<uuid8>
```

Example: `shiroha_sm_example-c1b49590`

- `component-stem`: filename without extension
- `uuid8`: first 8 chars of UUID v4

**Why**: Allows multiple instances of the same component to run simultaneously with distinct identities.

### Component Path Mapping

```rust
pub struct Daemon {
    pub task_manager: TaskManager,
    pub component_paths: Arc<RwLock<HashMap<String, PathBuf>>>,
    // ...
}
```

- `component_paths`: `task_id -> PathBuf` mapping
- Used by control interface to return component source in status queries
- Kept in sync with task lifecycle (no cleanup needed — tasks live until daemon stops)

### Loading Pattern

```rust
async fn load_components(&self, paths: &[PathBuf]) -> Result<usize> {
    let engine = self.create_wasm_engine()?;
    let mut success_count = 0;

    for path in paths {
        match self.load_single_component(&engine, path).await {
            Ok(task_id) => {
                info!("Task created: id={}, component={}", task_id, path.display());
                success_count += 1;
            }
            Err(e) => {
                error!("Failed to load component {}: {}", path.display(), e);
                // Continue loading remaining components
            }
        }
    }

    if success_count == 0 {
        anyhow::bail!("All components failed to load");
    }

    Ok(success_count)
}
```

**Key decisions**:
- Partial failure is tolerated — if 1/3 components fail, the other 2 run
- Bail only if all components fail
- Each component gets its own WASM adapter instance (no sharing)

---

## Control Interface

### Unix Socket Server

- **Protocol**: Line-delimited JSON (one Request/Response per line)
- **Location**: `/tmp/shirohad.sock` (configurable via `--socket`)
- **Concurrency**: One tokio task per client connection

```rust
pub async fn run_socket_server(
    task_manager: TaskManager,
    component_paths: Arc<RwLock<HashMap<String, PathBuf>>>,
    socket_path: PathBuf,
    cancel_token: CancellationToken,
) -> Result<()>
```

**Lifecycle**:
1. Clean up old socket file on startup
2. Accept connections in loop with `tokio::select!` + cancellation token
3. Spawn handler per connection
4. Clean up socket file on shutdown

### Protocol Layer

Defined in `shiroha-control` crate:

```rust
pub enum Request {
    ListTasks,
    SendEvent { task_id: String, event: String },
    TaskStatus { task_id: String },
}

pub struct Response {
    pub status: ResponseStatus,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}
```

**Error handling hierarchy**:
1. Connection refused → daemon not running
2. JSON parse error → protocol mismatch
3. `Response { status: Error }` → task not found / event send failed

---

## Concurrency Model

### Parallel Services

```rust
// Start socket server
let socket_handle = tokio::spawn(run_socket_server(
    daemon.task_manager.clone(),
    daemon.component_paths.clone(),
    daemon.socket_path.clone(),
    cancel_token.clone(),
));

// Run REPL or wait for shutdown
if args.repl {
    run_repl(...).await?;
} else {
    cancel_token.cancelled().await;
}

// Wait for socket server to stop
let _ = socket_handle.await;
```

**Coordination**:
- `CancellationToken` propagates shutdown signal to all tasks
- Socket server runs in background regardless of REPL/daemon mode
- Ctrl-C triggers cancellation → REPL exits → socket server stops → main returns

### Shared State

- `TaskManager` is `Clone` (wraps `Arc<RwLock<HashMap>>` internally)
- `component_paths` uses `Arc<RwLock<>>` for shared read/write access
- No explicit locking in application code — all accessed via `TaskManager::get_task()` or direct `RwLock` read

**Why this works**:
- Task creation is rare (startup only in current design)
- Queries are frequent but read-only
- RwLock allows concurrent reads without contention

---

## REPL Mode

### Architecture

```rust
pub async fn run_repl(
    task_manager: TaskManager,
    component_paths: Arc<RwLock<HashMap<String, PathBuf>>>,
    cancel_token: CancellationToken,
) -> Result<()> {
    loop {
        tokio::select! {
            biased;
            () = cancel_token.cancelled() => break,
            line = reader.next_line() => {
                // Handle command
            }
        }
    }
}
```

**Key patterns**:
- `tokio::select! { biased; ... }` ensures cancellation is checked first
- REPL runs in main task (blocking stdin read)
- Socket server runs concurrently in background
- `quit` command triggers `cancel_token.cancel()` → cascading shutdown

### Commands

- `status` — show all tasks (calls `TaskManager::list_tasks()` + `get_task()` per ID)
- `list-tasks` — show task IDs only
- `send-event <id> <event>` — dispatch event to task
- `help` — command listing
- `quit` / `exit` — graceful shutdown

---

## Pitfalls and Solutions

### ❌ Blocking in Tokio Runtime

**Problem**: Original `TaskHandle::get_state()` used `blocking_read()`:

```rust
pub fn get_state(&self) -> TaskState {
    self.state.blocking_read().clone()  // ❌ Panics in tokio runtime
}
```

**Error**:
```
Cannot block the current thread from within a runtime.
```

**Solution**: Make it async:

```rust
pub async fn get_state(&self) -> TaskState {
    self.state.read().await.clone()  // ✅
}

pub fn try_get_state(&self) -> Option<TaskState> {
    self.state.try_read().ok().map(|s| s.clone())  // ✅ Non-blocking alternative
}
```

**Rule**: Never use `blocking_*` methods on `tokio::sync` primitives inside async contexts.

### ❌ Integration Test Signature Drift

**Problem**: After adding `component_path` parameter to `create_task()`, all test callsites broke:

```rust
// Old
.create_task(id, def, invoker, guard)

// New
.create_task(id, def, invoker, guard, Some(path))
```

**Solution**: Search-and-replace all `create_task` calls in `tests/` directory.

**Prevention**: Use `cargo test` as the gate before manual verification — signature changes will fail early.

---

## Related Specs

- [WASM Component Integration](./wasm-component-integration.md) — how components are loaded
- [HSM Implementation Pattern](./hsm-implementation-pattern.md) — task runtime architecture
- [Async Patterns](./async-patterns.md) — tokio runtime guidelines
