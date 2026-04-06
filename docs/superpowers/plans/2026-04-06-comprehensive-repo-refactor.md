# Comprehensive Repository Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove repeated orchestration/query logic from Shiroha's server, storage, client, and CLI layers while preserving the existing gRPC and CLI behavior.

**Architecture:** Land the refactor in thin vertical slices. First consolidate shared test helpers, then centralize flow registry/cache state in `shirohad`, then extract job runtime and event-query logic, then tighten storage semantics with range-based queries, and finally shrink `shiroha-client`/`sctl` into cleaner helper-driven modules.

**Tech Stack:** Rust 1.94.1, Tokio, tonic, redb, serde_json, clap, cargo nextest

---

## Planned File Structure

**Create**

- `app/shirohad/src/flow_registry.rs`
- `app/shirohad/src/job_events.rs`
- `app/shirohad/src/job_runtime.rs`
- `app/shirohad/src/service_support.rs`
- `app/shirohad/src/test_support/flow_builders.rs`
- `app/shirohad/src/test_support/runtime_helpers.rs`
- `app/sctl/src/cli_support.rs`
- `app/sctl/src/command_runner.rs`
- `crate/shiroha-client/src/support.rs`

**Modify**

- `app/shirohad/src/main.rs`
- `app/shirohad/src/flow_service.rs`
- `app/shirohad/src/job_service.rs`
- `app/shirohad/src/server.rs`
- `app/shirohad/src/test_support.rs`
- `app/sctl/src/main.rs`
- `app/sctl/src/client.rs`
- `crate/shiroha-client/src/flow.rs`
- `crate/shiroha-client/src/job.rs`
- `crate/shiroha-client/src/lib.rs`
- `crate/shiroha-core/src/storage.rs`
- `crate/shiroha-store-redb/src/store.rs`

### Task 1: Consolidate Shared `shirohad` Test Fixtures

**Files:**
- Create: `app/shirohad/src/test_support/flow_builders.rs`
- Create: `app/shirohad/src/test_support/runtime_helpers.rs`
- Modify: `app/shirohad/src/test_support.rs`
- Modify: `app/shirohad/src/flow_service.rs`
- Modify: `app/shirohad/src/job_service.rs`
- Modify: `app/shirohad/src/server.rs`
- Test: `app/shirohad/src/test_support.rs`

- [ ] **Step 1: Write the failing shared-helper tests**

```rust
#[cfg(test)]
mod tests {
    use super::{approval_manifest_to, warning_manifest, wait_for_job};
    use shiroha_core::flow::StateKind;

    #[test]
    fn approval_manifest_to_uses_requested_terminal_state() {
        let manifest = approval_manifest_to("demo", "approved");

        assert_eq!(manifest.initial_state, "idle");
        assert_eq!(manifest.states.len(), 2);
        assert_eq!(manifest.states[1].name, "approved");
        assert_eq!(manifest.states[1].kind, StateKind::Terminal);
    }

    #[test]
    fn warning_manifest_has_looping_non_terminal_state() {
        let manifest = warning_manifest();

        assert_eq!(manifest.states[1].name, "loop");
        assert_eq!(manifest.states[1].kind, StateKind::Normal);
        assert_eq!(manifest.transitions[1].from, "loop");
        assert_eq!(manifest.transitions[1].to, "loop");
    }
}
```

- [ ] **Step 2: Run the targeted test to verify it fails**

Run: `cargo test -p shirohad test_support::tests::approval_manifest_to_uses_requested_terminal_state -- --exact`
Expected: FAIL because `approval_manifest_to` is not exported from shared test support yet.

- [ ] **Step 3: Implement the shared test helper modules**

```rust
// app/shirohad/src/test_support/flow_builders.rs
use shiroha_core::flow::{
    ActionDef, DispatchMode, FlowManifest, FlowWorld, StateDef, StateKind, TransitionDef,
};

pub(crate) fn approval_manifest_to(flow_id: &str, terminal_state: &str) -> FlowManifest {
    FlowManifest {
        id: flow_id.to_string(),
        host_world: FlowWorld::Sandbox,
        states: vec![
            StateDef {
                name: "idle".into(),
                kind: StateKind::Normal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
            StateDef {
                name: terminal_state.into(),
                kind: StateKind::Terminal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
        ],
        transitions: vec![TransitionDef {
            from: "idle".into(),
            to: terminal_state.into(),
            event: "approve".into(),
            guard: Some("allow".into()),
            action: Some("ship".into()),
            timeout: None,
        }],
        initial_state: "idle".into(),
        actions: vec![
            ActionDef {
                name: "ship".into(),
                dispatch: DispatchMode::Local,
                capabilities: Vec::new(),
            },
            ActionDef {
                name: "allow".into(),
                dispatch: DispatchMode::Local,
                capabilities: Vec::new(),
            },
        ],
    }
}

pub(crate) fn warning_manifest() -> FlowManifest {
    FlowManifest {
        id: "warning-demo".into(),
        host_world: FlowWorld::Sandbox,
        states: vec![
            StateDef {
                name: "idle".into(),
                kind: StateKind::Normal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
            StateDef {
                name: "loop".into(),
                kind: StateKind::Normal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
            StateDef {
                name: "done".into(),
                kind: StateKind::Terminal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
        ],
        transitions: vec![
            TransitionDef {
                from: "idle".into(),
                to: "loop".into(),
                event: "start".into(),
                guard: None,
                action: None,
                timeout: None,
            },
            TransitionDef {
                from: "loop".into(),
                to: "loop".into(),
                event: "spin".into(),
                guard: None,
                action: None,
                timeout: None,
            },
        ],
        initial_state: "idle".into(),
        actions: Vec::new(),
    }
}
```

```rust
// app/shirohad/src/test_support/runtime_helpers.rs
use tokio::time::{Duration, sleep, timeout};
use tonic::Request;

use crate::job_service::JobServiceImpl;
use shiroha_proto::shiroha_api::{GetJobRequest, GetJobResponse};

pub(crate) async fn wait_for_job(
    service: &JobServiceImpl,
    job_id: &str,
    expected_state: &str,
    expected_current_state: &str,
) -> GetJobResponse {
    timeout(Duration::from_millis(400), async {
        loop {
            let job = service
                .get_job(Request::new(GetJobRequest {
                    job_id: job_id.to_string(),
                }))
                .await
                .expect("get job")
                .into_inner();
            if job.state == expected_state && job.current_state == expected_current_state {
                break job;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("job should reach expected state")
}
```

```rust
// app/shirohad/src/test_support.rs
mod flow_builders;
mod runtime_helpers;

pub(crate) use flow_builders::{approval_manifest_to, warning_manifest};
pub(crate) use runtime_helpers::wait_for_job;
```

- [ ] **Step 4: Run the shared-helper tests and the affected `shirohad` regressions**

Run: `cargo nextest run -p shirohad approval_manifest_to_uses_requested_terminal_state warning_manifest_has_looping_non_terminal_state list_flow_versions_and_get_specific_version_work redeploy_keeps_existing_jobs_on_their_bound_flow_version`
Expected: PASS for all selected tests.

- [ ] **Step 5: Commit the fixture consolidation**

```bash
git add app/shirohad/src/test_support.rs app/shirohad/src/test_support/flow_builders.rs app/shirohad/src/test_support/runtime_helpers.rs app/shirohad/src/flow_service.rs app/shirohad/src/job_service.rs app/shirohad/src/server.rs
git commit -m "test: extract shared shirohad fixtures"
```

### Task 2: Centralize Flow Registry and Engine Cache Management

**Files:**
- Create: `app/shirohad/src/flow_registry.rs`
- Modify: `app/shirohad/src/main.rs`
- Modify: `app/shirohad/src/server.rs`
- Modify: `app/shirohad/src/flow_service.rs`
- Modify: `app/shirohad/src/job_service.rs`
- Modify: `app/shirohad/src/test_support.rs`
- Modify: `app/shirohad/src/test_support/runtime_helpers.rs`
- Test: `app/shirohad/src/flow_registry.rs`

- [ ] **Step 1: Write the failing flow-registry unit tests**

```rust
#[cfg(test)]
mod tests {
    use super::FlowRegistry;
    use shiroha_core::flow::{FlowManifest, FlowRegistration, FlowWorld, StateDef, StateKind};
    use uuid::Uuid;

    fn sample_registration(version: Uuid) -> FlowRegistration {
        FlowRegistration {
            flow_id: "demo".into(),
            version,
            manifest: FlowManifest {
                id: "demo".into(),
                host_world: FlowWorld::Sandbox,
                states: vec![StateDef {
                    name: "idle".into(),
                    kind: StateKind::Normal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                }],
                transitions: Vec::new(),
                initial_state: "idle".into(),
                actions: Vec::new(),
            },
            wasm_hash: format!("hash-{version}"),
        }
    }

    #[tokio::test]
    async fn register_version_updates_latest_when_version_is_newer() {
        let registry = FlowRegistry::default();
        let older = sample_registration(Uuid::from_u128(1));
        let newer = sample_registration(Uuid::from_u128(2));

        registry.register(older.clone()).await;
        registry.register(newer.clone()).await;

        let latest = registry.latest_flow(&older.flow_id).await.expect("latest flow");
        assert_eq!(latest.version, newer.version);
    }

    #[tokio::test]
    async fn remove_flow_clears_latest_and_versioned_entries() {
        let registry = FlowRegistry::default();
        let registration = sample_registration(Uuid::from_u128(7));

        registry.register(registration.clone()).await;
        registry.remove(&registration.flow_id).await;

        assert!(registry.latest_flow(&registration.flow_id).await.is_none());
        assert!(registry
            .versioned_flow(&registration.flow_id, registration.version)
            .await
            .is_none());
    }
}
```

- [ ] **Step 2: Run the flow-registry tests to verify they fail**

Run: `cargo test -p shirohad flow_registry::tests::register_version_updates_latest_when_version_is_newer -- --exact`
Expected: FAIL because `flow_registry.rs` does not exist yet.

- [ ] **Step 3: Implement the shared registry module and wire `ShirohaState` to use it**

```rust
// app/shirohad/src/flow_registry.rs
use std::collections::HashMap;
use std::sync::Arc;

use shiroha_core::flow::FlowRegistration;
use shiroha_engine::engine::StateMachineEngine;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Default)]
pub struct FlowRegistry {
    latest: Mutex<HashMap<String, FlowRegistration>>,
    latest_engines: Mutex<HashMap<String, Arc<StateMachineEngine>>>,
    versioned: Mutex<HashMap<(String, Uuid), FlowRegistration>>,
    versioned_engines: Mutex<HashMap<(String, Uuid), Arc<StateMachineEngine>>>,
}

impl FlowRegistry {
    pub async fn register(&self, registration: FlowRegistration) {
        let versioned_key = (registration.flow_id.clone(), registration.version);
        let versioned_engine = Arc::new(StateMachineEngine::new(registration.manifest.clone()));
        self.versioned.lock().await.insert(versioned_key.clone(), registration.clone());
        self.versioned_engines
            .lock()
            .await
            .insert(versioned_key, versioned_engine);

        let replace_latest = self
            .latest
            .lock()
            .await
            .get(&registration.flow_id)
            .is_none_or(|current| registration.version > current.version);
        if replace_latest {
            self.latest
                .lock()
                .await
                .insert(registration.flow_id.clone(), registration.clone());
            self.latest_engines
                .lock()
                .await
                .insert(
                    registration.flow_id.clone(),
                    Arc::new(StateMachineEngine::new(registration.manifest.clone())),
                );
        }
    }

    pub async fn latest_flow(&self, flow_id: &str) -> Option<FlowRegistration> {
        self.latest.lock().await.get(flow_id).cloned()
    }

    pub async fn versioned_flow(&self, flow_id: &str, version: Uuid) -> Option<FlowRegistration> {
        self.versioned
            .lock()
            .await
            .get(&(flow_id.to_string(), version))
            .cloned()
    }

    pub async fn versioned_engine(
        &self,
        flow_id: &str,
        version: Uuid,
    ) -> Option<Arc<StateMachineEngine>> {
        self.versioned_engines
            .lock()
            .await
            .get(&(flow_id.to_string(), version))
            .cloned()
    }

    pub async fn remove(&self, flow_id: &str) {
        self.latest.lock().await.remove(flow_id);
        self.latest_engines.lock().await.remove(flow_id);
        self.versioned
            .lock()
            .await
            .retain(|(candidate, _), _| candidate != flow_id);
        self.versioned_engines
            .lock()
            .await
            .retain(|(candidate, _), _| candidate != flow_id);
    }
}
```

```rust
// app/shirohad/src/server.rs
pub struct ShirohaState {
    pub storage: Arc<RedbStorage>,
    pub wasm_runtime: Arc<WasmRuntime>,
    pub module_cache: Arc<ModuleCache>,
    pub flow_registry: Arc<FlowRegistry>,
    pub(crate) job_locks: Arc<Mutex<HashMap<uuid::Uuid, Arc<Mutex<()>>>>>,
    pub job_manager: Arc<JobManager<RedbStorage>>,
    pub timer_wheel: Arc<TimerWheel>,
}
```

```rust
// app/shirohad/src/flow_service.rs
self.state
    .flow_registry
    .register(registration.clone())
    .await;
```

- [ ] **Step 4: Run the registry tests and the existing flow-binding regressions**

Run: `cargo nextest run -p shirohad register_version_updates_latest_when_version_is_newer remove_flow_clears_latest_and_versioned_entries list_flow_versions_and_get_specific_version_work redeploy_keeps_existing_jobs_on_their_bound_flow_version`
Expected: PASS for the new registry unit tests and the existing service regressions.

- [ ] **Step 5: Commit the registry extraction**

```bash
git add app/shirohad/src/flow_registry.rs app/shirohad/src/main.rs app/shirohad/src/server.rs app/shirohad/src/flow_service.rs app/shirohad/src/job_service.rs app/shirohad/src/test_support.rs
git commit -m "refactor: centralize flow registry state"
```

### Task 3: Extract Shared gRPC Support and Job Event Filtering

**Files:**
- Create: `app/shirohad/src/service_support.rs`
- Create: `app/shirohad/src/job_events.rs`
- Modify: `app/shirohad/src/main.rs`
- Modify: `app/shirohad/src/flow_service.rs`
- Modify: `app/shirohad/src/job_service.rs`
- Test: `app/shirohad/src/job_events.rs`

- [ ] **Step 1: Write the failing event-query tests**

```rust
#[cfg(test)]
mod tests {
    use super::{EventQuery, filter_events, validate_query};
    use shiroha_core::event::{EventKind, EventRecord};
    use uuid::Uuid;

    #[test]
    fn validate_query_rejects_since_id_and_timestamp_together() {
        let query = EventQuery {
            since_id: Some(Uuid::nil().to_string()),
            since_timestamp_ms: Some(7),
            limit: None,
            kind: Vec::new(),
        };

        let error = validate_query(&query).expect_err("query should be rejected");
        assert!(error.message().contains("cannot be used together"));
    }

    #[test]
    fn filter_events_applies_cursor_kind_and_limit() {
        let job_id = Uuid::now_v7();
        let kept = EventRecord {
            id: Uuid::from_u128(3),
            job_id,
            timestamp_ms: 30,
            kind: EventKind::Completed {
                final_state: "done".into(),
            },
        };
        let filtered = filter_events(
            vec![
                EventRecord {
                    id: Uuid::from_u128(1),
                    job_id,
                    timestamp_ms: 10,
                    kind: EventKind::Created {
                        flow_id: "flow".into(),
                        flow_version: Uuid::from_u128(9),
                        initial_state: "idle".into(),
                    },
                },
                EventRecord {
                    id: Uuid::from_u128(2),
                    job_id,
                    timestamp_ms: 20,
                    kind: EventKind::Transition {
                        event: "approve".into(),
                        from: "idle".into(),
                        to: "done".into(),
                        action: None,
                    },
                },
                kept.clone(),
            ],
            &EventQuery {
                since_id: Some(Uuid::from_u128(2).to_string()),
                since_timestamp_ms: None,
                limit: Some(1),
                kind: vec!["completed".into()],
            },
        )
        .expect("filter events");

        assert_eq!(filtered, vec![kept]);
    }
}
```

- [ ] **Step 2: Run the event-query test to verify it fails**

Run: `cargo test -p shirohad job_events::tests::validate_query_rejects_since_id_and_timestamp_together -- --exact`
Expected: FAIL because `job_events.rs` does not exist yet.

- [ ] **Step 3: Implement `service_support` and `job_events`, then delegate from the handlers**

```rust
// app/shirohad/src/service_support.rs
use shiroha_core::error::ShirohaError;
use tonic::Status;
use uuid::Uuid;

pub(crate) fn parse_uuid(input: &str) -> Result<Uuid, Status> {
    input
        .parse::<Uuid>()
        .map_err(|_| Status::invalid_argument(format!("invalid UUID: {input}")))
}

pub(crate) fn map_job_error(error: ShirohaError) -> Status {
    match error {
        ShirohaError::JobNotFound(_) => Status::not_found(error.to_string()),
        ShirohaError::InvalidJobState { .. } => Status::failed_precondition(error.to_string()),
        _ => Status::internal(error.to_string()),
    }
}
```

```rust
// app/shirohad/src/job_events.rs
use shiroha_core::event::{EventKind, EventRecord};
use tonic::Status;
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub(crate) struct EventQuery {
    pub since_id: Option<String>,
    pub since_timestamp_ms: Option<u64>,
    pub limit: Option<u32>,
    pub kind: Vec<String>,
}

pub(crate) fn validate_query(query: &EventQuery) -> Result<(), Status> {
    if query.since_id.is_some() && query.since_timestamp_ms.is_some() {
        return Err(Status::invalid_argument(
            "`since_id` and `since_timestamp_ms` cannot be used together",
        ));
    }
    if query.limit == Some(0) {
        return Err(Status::invalid_argument("`limit` must be greater than 0"));
    }
    if let Some(kind) = query.kind.iter().find(|kind| {
        !matches!(
            kind.as_str(),
            "created" | "transition" | "action_complete" | "paused" | "resumed" | "cancelled" | "completed"
        )
    }) {
        return Err(Status::invalid_argument(format!(
            "unknown event kind filter: {kind}"
        )));
    }
    Ok(())
}

pub(crate) fn filter_events(
    mut events: Vec<EventRecord>,
    query: &EventQuery,
) -> Result<Vec<EventRecord>, Status> {
    if let Some(since_id) = query.since_id.as_deref() {
        let cursor = since_id
            .parse::<Uuid>()
            .map_err(|_| Status::invalid_argument(format!("invalid UUID: {since_id}")))?;
        let Some(index) = events.iter().position(|event| event.id == cursor) else {
            return Err(Status::invalid_argument(format!(
                "event `{since_id}` not found in result set"
            )));
        };
        events.drain(..=index);
    }
    if let Some(since_timestamp_ms) = query.since_timestamp_ms {
        events.retain(|event| event.timestamp_ms > since_timestamp_ms);
    }
    if !query.kind.is_empty() {
        let kinds = query.kind.iter().map(String::as_str).collect::<std::collections::HashSet<_>>();
        events.retain(|event| kinds.contains(event_kind_name(&event.kind)));
    }
    if let Some(limit) = query.limit {
        events.truncate(limit as usize);
    }
    Ok(events)
}

fn event_kind_name(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::Created { .. } => "created",
        EventKind::Transition { .. } => "transition",
        EventKind::ActionComplete { .. } => "action_complete",
        EventKind::Paused => "paused",
        EventKind::Resumed => "resumed",
        EventKind::Cancelled => "cancelled",
        EventKind::Completed { .. } => "completed",
    }
}
```

- [ ] **Step 4: Run the new unit tests and the existing event-query regression**

Run: `cargo nextest run -p shirohad validate_query_rejects_since_id_and_timestamp_together filter_events_applies_cursor_kind_and_limit get_job_events_supports_cursor_kind_and_limit`
Expected: PASS for the new unit tests and the existing service-level event-query regression.

- [ ] **Step 5: Commit the shared gRPC/query support extraction**

```bash
git add app/shirohad/src/service_support.rs app/shirohad/src/job_events.rs app/shirohad/src/main.rs app/shirohad/src/flow_service.rs app/shirohad/src/job_service.rs
git commit -m "refactor: extract job event query support"
```

### Task 4: Start `job_runtime.rs` with Explicit Action Sequencing

**Files:**
- Create: `app/shirohad/src/job_runtime.rs`
- Modify: `app/shirohad/src/main.rs`
- Modify: `app/shirohad/src/job_service.rs`
- Test: `app/shirohad/src/job_runtime.rs`

- [ ] **Step 1: Add the failing `job_runtime` sequencing unit test**

```rust
#[cfg(test)]
mod tests {
    use super::action_sequence;

    #[test]
    fn action_sequence_orders_exit_transition_and_enter_hooks() {
        let sequence = action_sequence(
            Some("leave-idle".into()),
            Some("ship".into()),
            Some("enter-done".into()),
            "idle",
            "done",
        );

        assert_eq!(sequence.len(), 3);
        assert_eq!(sequence[0].name, "leave-idle");
        assert_eq!(sequence[0].state, "idle");
        assert_eq!(sequence[1].name, "ship");
        assert_eq!(sequence[1].state, "done");
        assert_eq!(sequence[2].name, "enter-done");
        assert_eq!(sequence[2].state, "done");
    }
}
```

- [ ] **Step 2: Run the unit test to verify it fails**

Run: `cargo test -p shirohad job_runtime::tests::action_sequence_orders_exit_transition_and_enter_hooks -- --exact`
Expected: FAIL because `job_runtime.rs` and `action_sequence` do not exist yet.

- [ ] **Step 3: Implement the job runtime coordinator and delegate from `JobServiceImpl`**

```rust
// app/shirohad/src/job_runtime.rs
pub(crate) struct ActionInvocation {
    pub(crate) name: String,
    pub(crate) state: String,
}

pub(crate) fn action_sequence(
    on_exit: Option<String>,
    transition_action: Option<String>,
    on_enter: Option<String>,
    from: &str,
    to: &str,
) -> Vec<ActionInvocation> {
    let mut sequence = Vec::new();
    if let Some(name) = on_exit {
        sequence.push(ActionInvocation {
            name,
            state: from.to_string(),
        });
    }
    if let Some(name) = transition_action {
        sequence.push(ActionInvocation {
            name,
            state: to.to_string(),
        });
    }
    if let Some(name) = on_enter {
        sequence.push(ActionInvocation {
            name,
            state: to.to_string(),
        });
    }
    sequence
}
```

```rust
// app/shirohad/src/job_service.rs
use crate::job_runtime::action_sequence;

let mut action_failures = Vec::new();
for invocation in action_sequence(on_exit, action, on_enter, &from, &to) {
    if let Some(message) = self
        .run_declared_action(
            &flow,
            &invocation.name,
            job.id,
            &invocation.state,
            payload.clone(),
        )
        .await?
    {
        action_failures.push(message);
    }
}
```

- [ ] **Step 4: Run the runtime regression set**

Run: `cargo nextest run -p shirohad action_sequence_orders_exit_transition_and_enter_hooks redeploy_keeps_existing_jobs_on_their_bound_flow_version`
Expected: PASS for the new sequencing unit test and the existing version-binding regression.

- [ ] **Step 5: Commit the runtime extraction**

```bash
git add app/shirohad/src/job_runtime.rs app/shirohad/src/main.rs app/shirohad/src/job_service.rs
git commit -m "refactor: extract job action sequencing"
```

### Task 5: Tighten Storage Semantics with Flow-Version Range Queries

**Files:**
- Modify: `crate/shiroha-core/src/storage.rs`
- Modify: `crate/shiroha-store-redb/src/store.rs`
- Modify: `app/shirohad/src/flow_service.rs`
- Test: `crate/shiroha-store-redb/src/store.rs`

- [ ] **Step 1: Add the storage tests**

```rust
#[tokio::test]
async fn list_flow_versions_for_returns_only_matching_flow() {
    let path = temp_db_path("flow-versions-for");
    let storage = RedbStorage::new(&path).expect("open db");
    let first = sample_flow_registration();
    let second = FlowRegistration {
        flow_id: "other".into(),
        version: Uuid::now_v7(),
        ..sample_flow_registration()
    };

    storage.save_flow(&first).await.expect("save first");
    storage.save_flow(&second).await.expect("save second");

    let versions = storage
        .list_flow_versions_for(&first.flow_id)
        .await
        .expect("list flow versions for");

    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].flow_id, first.flow_id);
}

#[tokio::test]
async fn get_events_ignores_records_for_other_jobs_with_neighboring_keys() {
    let path = temp_db_path("event-prefix");
    let flow = sample_flow_registration();
    let first_job = sample_job(&flow);
    let second_job = sample_job(&flow);
    let storage = RedbStorage::new(&path).expect("open db");

    storage.save_job(&first_job).await.expect("save first job");
    storage.save_job(&second_job).await.expect("save second job");
    storage
        .append_event(&EventRecord {
            id: Uuid::from_u128(1),
            job_id: first_job.id,
            timestamp_ms: 10,
            kind: EventKind::Paused,
        })
        .await
        .expect("append first event");
    storage
        .append_event(&EventRecord {
            id: Uuid::from_u128(2),
            job_id: second_job.id,
            timestamp_ms: 20,
            kind: EventKind::Cancelled,
        })
        .await
        .expect("append second event");

    let events = storage.get_events(first_job.id).await.expect("get events");

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].job_id, first_job.id);
}
```

- [ ] **Step 2: Run the RED check for the new per-flow version query**

Run: `cargo test -p shiroha-store-redb store::tests::list_flow_versions_for_returns_only_matching_flow -- --exact`
Expected: FAIL because `Storage::list_flow_versions_for` is not defined yet.

- [ ] **Step 3: Extend the storage trait and implement range-based reads in redb**

```rust
// crate/shiroha-core/src/storage.rs
pub trait Storage: Send + Sync {
    fn list_flow_versions(&self) -> impl Future<Output = Result<Vec<FlowRegistration>>> + Send;
    fn list_flow_versions_for(
        &self,
        flow_id: &str,
    ) -> impl Future<Output = Result<Vec<FlowRegistration>>> + Send;
    fn get_events(&self, job_id: Uuid) -> impl Future<Output = Result<Vec<EventRecord>>> + Send;
}
```

```rust
// crate/shiroha-store-redb/src/store.rs
async fn list_flow_versions_for(&self, flow_id: &str) -> Result<Vec<FlowRegistration>> {
    let txn = self.db.begin_read().map_err(s)?;
    let table = txn.open_table(FLOW_VERSIONS_TABLE).map_err(s)?;
    let start = format!("{flow_id}\u{0}");
    let end = format!("{flow_id}\u{0}\u{10ffff}");
    let mut flows = Vec::new();
    for entry in table.range(start.as_str()..=end.as_str()).map_err(s)? {
        let (_, value) = entry.map_err(s)?;
        flows.push(serde_json::from_slice(value.value()).map_err(s)?);
    }
    Ok(flows)
}

async fn get_events(&self, job_id: Uuid) -> Result<Vec<EventRecord>> {
    let txn = self.db.begin_read().map_err(s)?;
    let table = txn.open_table(EVENTS_TABLE).map_err(s)?;
    let start = Self::event_key(job_id, Uuid::nil());
    let end = Self::event_key(job_id, Uuid::from_u128(u128::MAX));
    let mut events = Vec::new();
    for entry in table.range(start.as_slice()..=end.as_slice()).map_err(s)? {
        let (_, value) = entry.map_err(s)?;
        events.push(serde_json::from_slice(value.value()).map_err(s)?);
    }
    events.sort_by_key(|event: &EventRecord| event.timestamp_ms);
    Ok(events)
}
```

```rust
// app/shirohad/src/flow_service.rs
let flows = self
    .state
    .storage
    .list_flow_versions_for(&flow_id)
    .await
    .map_err(|e| Status::internal(e.to_string()))?;
```

- [ ] **Step 4: Run the storage and flow-version query regression set**

Run: `cargo nextest run -p shiroha-store-redb -p shirohad list_flow_versions_for_returns_only_matching_flow get_events_ignores_records_for_other_jobs_with_neighboring_keys list_flow_versions_and_get_specific_version_work`
Expected: PASS for the two new storage regressions and the existing flow-service query regression.

- [ ] **Step 5: Commit the storage query tightening**

```bash
git add crate/shiroha-core/src/storage.rs crate/shiroha-store-redb/src/store.rs app/shirohad/src/flow_service.rs
git commit -m "refactor: tighten flow and event storage queries"
```

### Task 6: Shrink `shiroha-client` and `sctl` Around Shared Helpers

**Files:**
- Create: `crate/shiroha-client/src/support.rs`
- Create: `app/sctl/src/cli_support.rs`
- Create: `app/sctl/src/command_runner.rs`
- Modify: `crate/shiroha-client/src/lib.rs`
- Modify: `crate/shiroha-client/src/flow.rs`
- Modify: `crate/shiroha-client/src/job.rs`
- Modify: `app/sctl/src/main.rs`
- Modify: `app/sctl/src/client.rs`
- Test: `crate/shiroha-client/src/support.rs`
- Test: `app/sctl/src/cli_support.rs`

- [ ] **Step 1: Write the failing helper tests**

```rust
// crate/shiroha-client/src/support.rs
#[cfg(test)]
mod tests {
    use super::sort_job_details;
    use crate::JobDetails;

    #[test]
    fn sort_job_details_orders_by_flow_then_job_id() {
        let mut jobs = vec![
            JobDetails {
                job_id: "job-b".into(),
                flow_id: "flow-a".into(),
                state: "running".into(),
                current_state: "idle".into(),
                flow_version: "v1".into(),
                context_bytes: None,
            },
            JobDetails {
                job_id: "job-a".into(),
                flow_id: "flow-a".into(),
                state: "running".into(),
                current_state: "idle".into(),
                flow_version: "v1".into(),
                context_bytes: None,
            },
        ];

        sort_job_details(&mut jobs);

        assert_eq!(jobs[0].job_id, "job-a");
        assert_eq!(jobs[1].job_id, "job-b");
    }
}
```

```rust
// app/sctl/src/cli_support.rs
#[cfg(test)]
mod tests {
    use super::parse_positive_usize;

    #[test]
    fn parse_positive_usize_rejects_zero() {
        assert_eq!(
            parse_positive_usize("0"),
            Err("value must be greater than 0".to_string())
        );
    }
}
```

- [ ] **Step 2: Run the helper tests to verify they fail**

Run: `cargo test -p shiroha-client support::tests::sort_job_details_orders_by_flow_then_job_id -- --exact`
Expected: FAIL because `support.rs` does not exist yet.

- [ ] **Step 3: Implement shared helper modules and trim the entrypoints**

```rust
// crate/shiroha-client/src/support.rs
use shiroha_proto::shiroha_api::{GetFlowRequest, GetJobResponse};

use crate::JobDetails;

pub(crate) fn bound_flow_request(job: &JobDetails) -> GetFlowRequest {
    GetFlowRequest {
        flow_id: job.flow_id.clone(),
        version: Some(job.flow_version.clone()),
    }
}

pub(crate) fn sort_jobs(jobs: &mut [GetJobResponse]) {
    jobs.sort_by(|left, right| {
        left.flow_id
            .cmp(&right.flow_id)
            .then_with(|| left.job_id.cmp(&right.job_id))
    });
}

pub(crate) fn sort_job_details(jobs: &mut [JobDetails]) {
    jobs.sort_by(|left, right| {
        left.flow_id
            .cmp(&right.flow_id)
            .then_with(|| left.job_id.cmp(&right.job_id))
    });
}
```

```rust
// app/sctl/src/cli_support.rs
use std::path::{Path, PathBuf};

pub(crate) fn parse_positive_usize(input: &str) -> Result<usize, String> {
    parse_positive(input)
}

pub(crate) fn parse_positive_u32(input: &str) -> Result<u32, String> {
    parse_positive(input)
}

fn parse_positive<T>(input: &str) -> Result<T, String>
where
    T: std::str::FromStr + PartialEq + Default,
{
    let value = input
        .parse::<T>()
        .map_err(|_| format!("invalid value: {input}"))?;
    if value == T::default() {
        return Err("value must be greater than 0".to_string());
    }
    Ok(value)
}

pub(crate) fn write_completion_script(path: &Path, script: &[u8]) -> anyhow::Result<()> {
    std::fs::write(path, script)?;
    Ok(())
}
```

```rust
// app/sctl/src/main.rs
mod cli_support;
mod command_runner;

fn main() -> anyhow::Result<()> {
    command_runner::run()
}
```

- [ ] **Step 4: Run the helper and CLI regression set**

Run: `cargo nextest run -p shiroha-client -p sctl sort_job_details_orders_by_flow_then_job_id bound_flow_request_uses_job_bound_version parse_positive_usize_rejects_zero complete_command_emits_bash_script jobs_to_json_value_returns_array`
Expected: PASS for the new helper tests and the selected existing client/CLI regressions.

- [ ] **Step 5: Commit the client/CLI cleanup**

```bash
git add crate/shiroha-client/src/support.rs crate/shiroha-client/src/lib.rs crate/shiroha-client/src/flow.rs crate/shiroha-client/src/job.rs app/sctl/src/cli_support.rs app/sctl/src/command_runner.rs app/sctl/src/main.rs app/sctl/src/client.rs
git commit -m "refactor: shrink client and cli entrypoints"
```

### Task 7: Final Verification and Cleanup

**Files:**
- Modify: files changed in Tasks 1-6
- Test: workspace-wide

- [ ] **Step 1: Run the workspace compilation check**

```bash
cargo check --workspace
```

Expected: exit code 0.

- [ ] **Step 2: Run the strict linter pass**

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Expected: exit code 0 with no warnings.

- [ ] **Step 3: Run the full test suite**

```bash
cargo nextest run --all-features --no-tests=warn
```

Expected: all non-ignored tests PASS.

- [ ] **Step 4: Run formatting and repository checks**

```bash
just fmt
```

Expected: exit code 0 and no remaining formatting/pre-commit failures.

- [ ] **Step 5: Commit the final repository-wide refactor**

```bash
git add app/shirohad app/sctl crate/shiroha-client crate/shiroha-core crate/shiroha-store-redb docs/superpowers/plans/2026-04-06-comprehensive-repo-refactor.md
git commit -m "refactor: consolidate server and client boundaries"
```
