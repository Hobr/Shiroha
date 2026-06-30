# HSM (Hierarchical State Machine) Implementation Pattern

> Contracts and patterns for implementing hierarchical state machines with do-activity lifecycle management.

---

## 1. Scope / Trigger

**Trigger**: Implementing state machine runtime with nested states, entry/exit actions, do-activities, and event-driven transitions.

**Applies to**: State machine engines, workflow systems, actor-based task systems.

---

## 2. Core Architecture

### Layer Separation

```
┌─────────────────────────────────────┐
│  TaskManager (control plane)        │  ← External API
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│  Task (actor instance)               │  ← Per-instance runtime
│  - Event mailbox (mpsc)              │
│  - RTC event loop                    │
│  - Active state tracking             │
│  - History storage                   │
└──────────────┬──────────────────────┘
               │
┌──────────────▼──────────────────────┐
│  StateMachineDef (IR)                │  ← Pure data (no runtime)
│  - States, transitions, actions      │
└─────────────────────────────────────┘
```

**Contract**:
- **IR layer** (`StateMachineDef`) contains only data structures and validation
- **Runtime layer** (`Task`) contains execution logic and state tracking
- **Control layer** (`TaskManager`) manages task lifecycle (create/destroy/lookup)

---

## 3. Signatures

### TaskManager API

```rust
pub struct TaskManager {
    tasks: Arc<Mutex<HashMap<TaskId, TaskHandle>>>,
}

impl TaskManager {
    pub async fn create_task(
        &self,
        def: Arc<StateMachineDef>,
        adapter: Arc<dyn Adapter>,
        invoker: Arc<dyn ActionInvoker>,
        guard_evaluator: Arc<dyn GuardEvaluator>,
        authorizer: Arc<dyn Authorizer>,
    ) -> Result<TaskHandle, String>;

    pub async fn send_event(&self, task_id: TaskId, event: Event) -> Result<(), String>;

    pub async fn get_state(&self, task_id: TaskId) -> Result<StateId, String>;

    pub async fn shutdown_task(&self, task_id: TaskId) -> Result<(), String>;
}
```

### Task Actor

```rust
pub struct Task {
    id: TaskId,
    def: Arc<StateMachineDef>,
    current: StateId,
    history: HashMap<StateId, HistoryEntry>,
    mailbox: UnboundedReceiver<Event>,
    do_handles: HashMap<StateId, JoinHandle<()>>,
    // trait dependencies...
}

impl Task {
    async fn run(mut self);
    async fn process_event(&mut self, event: Event);
}
```

### TaskHandle (Clone-able Sender)

```rust
pub struct TaskHandle {
    id: TaskId,
    sender: UnboundedSender<Event>,
}

impl TaskHandle {
    pub fn id(&self) -> TaskId { self.id }
    pub fn send(&self, event: Event) -> Result<(), String>;
}
```

**Contract**: `TaskHandle` is clone-able and can be distributed; sending is non-blocking.

---

## 4. Run-to-Completion (RTC) Event Loop

### Pattern

```rust
async fn run(mut self) {
    while let Some(event) = self.mailbox.recv().await {
        self.process_event(event).await;
        // Event processed to completion before next event
    }
}
```

**Rules**:
1. **One event at a time**: Process current event fully before fetching next
2. **Atomic transitions**: Entry/exit/action sequence cannot be interrupted
3. **No event loss**: Mailbox is unbounded; events queue if processing is slow

### Transition Sequence

```
1. Select transition (first with satisfied guard)
2. Exit cascade (LCA → current, innermost to outermost)
3. Execute transition action
4. Entry cascade (LCA → target, outermost to innermost)
5. Update current state
6. Update history storage
7. Spawn do-activity (if target has one)
```

**Why LCA (Least Common Ancestor)?**:
- Transitioning `A.B.C → A.D` should exit `C`, `B` but NOT `A` (common parent)
- Avoids redundant exit/entry of shared ancestors

---

## 5. Do-Activity Lifecycle Management

### Pattern

```rust
async fn enter_state(&mut self, state_id: StateId) {
    let state = &self.def.states[state_id];

    // Run entry action (synchronous)
    if let Some(entry) = &state.entry {
        self.invoker.invoke_sync(/* ... */).await;
    }

    // Spawn do-activity (asynchronous task)
    if let Some(do_action) = &state.do_activity {
        let handle = tokio::spawn(async move {
            self.invoker.invoke_do(/* ... */).await;
            // Send completion event when done
        });
        self.do_handles.insert(state_id, handle);
    }
}

async fn exit_state(&mut self, state_id: StateId) {
    let state = &self.def.states[state_id];

    // Cancel do-activity (if running)
    if let Some(handle) = self.do_handles.remove(&state_id) {
        handle.abort();  // Cancels the task
    }

    // Run exit action (synchronous)
    if let Some(exit) = &state.exit {
        self.invoker.invoke_sync(/* ... */).await;
    }
}
```

**Contract**:
- **Entry action**: Runs synchronously before do-activity spawns
- **Do-activity**: Runs as independent tokio task, can be long-running
- **Exit action**: Runs synchronously after do-activity is cancelled
- **Cancellation**: Immediate (`abort()`), no graceful shutdown in MVP

---

## 6. History Storage

### Shallow History

```rust
pub enum HistoryEntry {
    Shallow(StateId),  // Direct child state only
    Deep(Vec<StateId>), // Full path (for orthogonal regions)
}

// Example: Parent has children A, B, C
// Last active: A → store Shallow(A)
// Exit parent → store history
// Re-enter parent with history → restore A
```

### Deep History (MVP: Single Path)

```rust
// Example: A contains B, B contains C
// Active path: A → B → C
// Store: Deep([B, C])
// Re-enter A with deep history → restore B → restore C
```

**Limitation**: MVP stores only one path (no orthogonal regions).

---

## 7. Validation & Error Matrix

| Condition | Error | Prevention |
|-----------|-------|------------|
| Event sent to non-existent task | `send_event` returns `Err("Task not found")` | Validate `TaskId` before sending |
| No transition matches event | No error, event ignored | Guards can catch unexpected events |
| Do-activity panics | Task continues (panic isolated in spawned task) | Use `std::panic::catch_unwind` in invoker if needed |
| Circular state nesting | IR validation fails at definition load | `StateMachineDef::validate()` detects cycles |
| Invalid history reference | IR validation fails | Validate parent-child relationships |

---

## 8. Good/Base/Bad Cases

### Good: Clean Separation of Concerns

```rust
// IR (data only)
pub struct StateMachineDef {
    pub states: Vec<State>,
    pub initial: StateId,
}

// Runtime (execution only)
pub struct Task {
    def: Arc<StateMachineDef>,  // Reference to immutable IR
    current: StateId,            // Mutable runtime state
}
```

**Benefit**: IR can be shared across multiple task instances; runtime state is per-instance.

### Base: Stub Implementations for Testing

```rust
struct StubActionInvoker;

#[async_trait]
impl ActionInvoker for StubActionInvoker {
    async fn invoke_sync(&self, ctx: ActionContext) -> ActionResult {
        Ok(())  // No-op for testing state machine logic
    }

    async fn invoke_do(&self, ctx: ActionContext) -> ActionResult {
        tokio::time::sleep(Duration::from_millis(10)).await;
        Ok(())
    }
}
```

**Usage**: Validate state machine semantics without real action implementations.

### Bad: Mixing IR and Runtime State

```rust
// ❌ Bad: Mutable state in IR
pub struct StateMachineDef {
    pub states: Vec<State>,
    pub current: StateId,  // Runtime state mixed with definition
}
```

**Problem**: Cannot share definition across multiple instances; breaks immutability.

---

## 9. Tests Required

### Unit Tests (State Machine Logic)

```rust
#[tokio::test]
async fn test_simple_transition() {
    // Setup: IR with states A → B on event "go"
    // Action: Send event "go"
    // Assert: Current state is B
}

#[tokio::test]
async fn test_nested_entry_exit_order() {
    // Setup: States A contains B, B contains C
    // Action: Transition A.B.C → A.D
    // Assert: Exit order: C, B; Entry order: D
}

#[tokio::test]
async fn test_do_activity_cancellation() {
    // Setup: State A has do-activity
    // Action: Enter A, then exit A before do completes
    // Assert: Do-activity task is aborted
}

#[tokio::test]
async fn test_history_restore() {
    // Setup: Parent state with history, child A
    // Action: Enter parent → A, exit parent, re-enter with history
    // Assert: Restored to A (not initial child)
}
```

### Integration Tests (TaskManager)

```rust
#[tokio::test]
async fn test_task_creation_and_event_send() {
    let manager = TaskManager::new();
    let handle = manager.create_task(/* ... */).await.unwrap();
    handle.send(Event::new("test")).unwrap();
    let state = manager.get_state(handle.id()).await.unwrap();
    assert_eq!(state, expected_state);
}
```

---

## 10. Wrong vs Correct

### Wrong: Blocking Event Loop

```rust
// ❌ Bad: Synchronous blocking in event loop
async fn run(mut self) {
    while let Some(event) = self.mailbox.recv().await {
        std::thread::sleep(Duration::from_secs(1));  // Blocks executor!
        self.process_event(event).await;
    }
}
```

**Problem**: Blocks tokio executor; no other tasks can run.

### Correct: Async Event Loop

```rust
// ✅ Good: Fully async
async fn run(mut self) {
    while let Some(event) = self.mailbox.recv().await {
        self.process_event(event).await;  // Yields to executor
    }
}
```

---

## 11. Design Decisions

### Decision: Unbounded Mailbox

**Context**: Should task mailbox use bounded or unbounded channel?

**Decision**: Use `tokio::sync::mpsc::unbounded_channel`.

**Why**:
- External systems may send events faster than task can process
- Bounded channel would block sender (backpressure not desirable for event-driven systems)
- Risk: Memory growth if events arrive faster than processing (mitigated by monitoring)

**Tradeoff**: Memory usage vs. non-blocking sends.

### Decision: Immediate Do-Activity Cancellation

**Context**: Should do-activities have graceful shutdown?

**Options**:
1. Immediate abort (`JoinHandle::abort()`)
2. Graceful shutdown (send cancel signal, wait for acknowledgment)

**Decision**: Immediate abort for MVP.

**Why**:
- Simpler implementation
- State machine semantics: exit means "stop immediately"
- Graceful shutdown can be added later via custom action logic

**Future**: v0.3.0+ can add "on-cancel" hook if needed.

### Decision: Single History Path (MVP)

**Context**: Should deep history support orthogonal regions?

**Decision**: MVP stores only one active path (no orthogonal regions).

**Why**:
- Orthogonal regions add significant complexity
- Most MVP use cases have single active path
- IR already reserves `State::ortho` field for future extension

**Extension Path**: v0.3.0+ can implement orthogonal region history by storing multiple paths.

---

## 12. Common Mistakes

### Mistake: Forgetting to Cancel Do-Activities

**Symptom**: Do-activities continue running after state exit, causing resource leaks.

**Fix**: Always call `handle.abort()` in `exit_state()`.

```rust
// ✅ Correct
async fn exit_state(&mut self, state_id: StateId) {
    if let Some(handle) = self.do_handles.remove(&state_id) {
        handle.abort();  // Cancel before exit action
    }
    // Run exit action...
}
```

### Mistake: Mutating IR During Runtime

**Symptom**: Changes to `StateMachineDef` affect all task instances.

**Fix**: Store IR as `Arc<StateMachineDef>` (immutable shared reference).

```rust
// ✅ Correct
pub struct Task {
    def: Arc<StateMachineDef>,  // Immutable
    current: StateId,            // Mutable per-instance state
}
```

### Mistake: Not Using LCA for Transitions

**Symptom**: Redundant exit/entry of common ancestor states.

**Fix**: Calculate LCA (Least Common Ancestor) and only exit/enter states between LCA and source/target.

```rust
// Transition: A.B.C → A.D
// LCA: A
// Exit path: C → B (stop at LCA)
// Entry path: D (start from LCA)
```

---

## 13. Extensibility

### Adding Orthogonal Regions (Future)

1. Update `State` to support multiple active children (`ortho: Vec<StateId>`)
2. Store multiple paths in `HistoryEntry::Deep(Vec<Vec<StateId>>)`
3. Process events across all active regions (broadcast)

### Adding Event Priorities (Future)

1. Replace `mpsc::unbounded_channel` with priority queue
2. Add `Event::priority` field
3. Process high-priority events before low-priority

### Adding Deferred Events (Future)

1. Store unprocessed events in per-state queue
2. Re-inject when state becomes active again

---

## Related

- [Rust Workspace Structure](./rust-workspace-structure.md) — Layer separation (IR vs runtime)
- [Error Handling](./error-handling.md) — Error propagation in actions
- [Code Reuse Thinking Guide](../guides/code-reuse-thinking-guide.md) — Avoid duplicating state machine logic
