# Async Patterns

**Purpose**: Document tokio runtime guidelines and common async patterns in the Shiroha codebase.

---

## Tokio Runtime Rules

### ❌ Never Block Inside Async Context

**Problem**:

```rust
pub fn get_state(&self) -> TaskState {
    self.state.blocking_read().clone()  // ❌ Panics
}
```

**Error**:
```
thread 'tokio-rt-worker' panicked at crates/engine/src/task.rs:59:20:
Cannot block the current thread from within a runtime.
This happens because a function attempted to block the current thread
while the thread is being used to drive asynchronous tasks.
```

**Why it fails**:
- `blocking_read()` / `blocking_write()` / `blocking_lock()` assume a thread pool where blocking is acceptable
- Tokio runtime threads must remain responsive to poll pending futures
- Blocking a runtime thread starves other tasks

**Solution**: Use async equivalents:

```rust
// ✅ Good: async method
pub async fn get_state(&self) -> TaskState {
    self.state.read().await.clone()
}

// ✅ Good: try without blocking
pub fn try_get_state(&self) -> Option<TaskState> {
    self.state.try_read().ok().map(|s| s.clone())
}
```

### When to Use `blocking_*`

**Only** in:
- `#[tokio::main]` before entering async runtime
- `tokio::task::spawn_blocking(|| { ... })` dedicated blocking tasks
- Non-async functions that will never be called from async context

**Never** in:
- `async fn` bodies
- Callbacks passed to async code
- Any code reachable from `#[tokio::main]`

---

## State Access Patterns

### Shared Read-Write State

```rust
pub struct TaskHandle {
    state: Arc<RwLock<TaskState>>,
    // ...
}
```

**Pattern**: Use `Arc<RwLock<T>>` for state shared across tasks.

**Access**:
```rust
// Read (async)
let state = handle.state.read().await;
println!("Current: {}", state.current_state);

// Write (async)
let mut state = handle.state.write().await;
state.current_state = "Processing".to_string();

// Try-read (non-blocking)
if let Ok(state) = handle.state.try_read() {
    println!("Current: {}", state.current_state);
}
```

**Why `RwLock` over `Mutex`**:
- Multiple concurrent readers allowed
- Only one writer at a time
- State reads are frequent (status queries), writes are rare (transitions)

### Clone-on-Access Pattern

```rust
pub async fn get_state(&self) -> TaskState {
    self.state.read().await.clone()
}
```

**Why clone**:
- Avoids holding lock across await points
- `TaskState` is `Clone` and small (few fields)
- Caller gets a snapshot, lock is released immediately

**Alternative** (if T is expensive to clone):
```rust
pub async fn with_state<F, R>(&self, f: F) -> R
where
    F: FnOnce(&TaskState) -> R,
{
    let state = self.state.read().await;
    f(&state)
}
```

---

## Graceful Shutdown Pattern

### CancellationToken Pattern

```rust
use tokio_util::sync::CancellationToken;

let cancel_token = CancellationToken::new();

// Worker task
let worker_token = cancel_token.clone();
let worker_handle = tokio::spawn(async move {
    loop {
        tokio::select! {
            biased;
            () = worker_token.cancelled() => break,
            result = do_work() => {
                // Process result
            }
        }
    }
});

// Signal handler
let signal_token = cancel_token.clone();
tokio::spawn(async move {
    signal::ctrl_c().await.ok();
    signal_token.cancel();
});

// Main task
cancel_token.cancelled().await;
let _ = worker_handle.await;
```

**Key points**:
- `tokio::select! { biased; ... }` checks cancellation first
- All tasks share clones of the same token
- `.cancelled().await` blocks until someone calls `.cancel()`
- Cascade: main cancels → workers see it → they exit → main awaits them

### Multiple Background Services

```rust
// Start services
let socket_handle = tokio::spawn(run_socket_server(..., cancel_token.clone()));
let repl_handle = tokio::spawn(run_repl(..., cancel_token.clone()));

// Wait for any to request shutdown
tokio::select! {
    _ = cancel_token.cancelled() => {},
    _ = signal::ctrl_c() => cancel_token.cancel(),
}

// Wait for all to stop
let _ = tokio::join!(socket_handle, repl_handle);
```

**Pattern**: Spawn all services, then `select!` on shutdown signals, then `join!` all handles.

---

## Channel Patterns

### MPSC for Task Communication

```rust
pub struct TaskHandle {
    sender: mpsc::Sender<Event>,
    // ...
}

impl TaskHandle {
    pub fn send(&self, event: Event) -> anyhow::Result<()> {
        self.sender
            .send(event)
            .map_err(|_| anyhow::anyhow!("Task mailbox closed"))?;
        Ok(())
    }
}
```

**Why `mpsc::Sender` not `mpsc::UnboundedSender`**:
- Bounded channel (default capacity) provides backpressure
- If task is slow, sender blocks instead of accumulating infinite events
- In practice, events are rare (user-triggered), so blocking is acceptable

**When to use unbounded**:
- High-frequency logging/telemetry where dropping is acceptable
- Fire-and-forget notifications with no backpressure needed

### Actor Pattern (Task Loop)

```rust
async fn task_loop(
    mut receiver: mpsc::Receiver<Event>,
    state: Arc<RwLock<TaskState>>,
) {
    while let Some(event) = receiver.recv().await {
        // Process event, update state
        let mut s = state.write().await;
        s.current_state = compute_next_state(&s.current_state, &event);
    }
}
```

**Lifecycle**:
- Loop runs until channel closes (all senders dropped)
- State updates are serialized (one event at a time)
- No explicit shutdown signal needed — dropping `TaskHandle` closes the channel

---

## Common Mistakes

### ❌ Holding Lock Across Await

```rust
let state = self.state.read().await;
let result = fetch_data(&state.id).await;  // ❌ Lock held during I/O
process(result);
```

**Why bad**: Lock is held while waiting for I/O, blocking other readers.

**Fix**:
```rust
let id = {
    let state = self.state.read().await;
    state.id.clone()
};  // Lock released here
let result = fetch_data(&id).await;  // ✅ Lock not held
process(result);
```

### ❌ Forgetting `biased` in Select

```rust
tokio::select! {
    () = cancel_token.cancelled() => break,
    result = work() => process(result),
}
```

**Problem**: Without `biased`, branches are checked in random order. Cancellation might be delayed by one iteration.

**Fix**:
```rust
tokio::select! {
    biased;  // ✅ Check branches in order
    () = cancel_token.cancelled() => break,
    result = work() => process(result),
}
```

### ❌ Spawning Without Joining

```rust
tokio::spawn(async {
    run_background_task().await;
});
// Daemon exits immediately, task is killed
```

**Fix**: Keep handle and await it:
```rust
let handle = tokio::spawn(async {
    run_background_task().await;
});
// ... do other work ...
let _ = handle.await;
```

---

## Related Specs

- [Daemon Architecture](./daemon-architecture.md) — concurrency model for shirohad
- [HSM Implementation Pattern](./hsm-implementation-pattern.md) — task actor pattern
