use std::collections::HashMap;
use std::sync::Arc;

use shiroha_core::flow::FlowRegistration;
use shiroha_engine::engine::StateMachineEngine;
use tokio::sync::Mutex;
use uuid::Uuid;

pub struct FlowRegistry {
    inner: Mutex<FlowRegistryInner>,
}

struct FlowRegistryInner {
    latest: HashMap<String, FlowRegistration>,
    versioned: HashMap<(String, Uuid), FlowRegistration>,
    latest_engines: HashMap<String, Arc<StateMachineEngine>>,
    versioned_engines: HashMap<(String, Uuid), Arc<StateMachineEngine>>,
}

impl FlowRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(FlowRegistryInner {
                latest: HashMap::new(),
                versioned: HashMap::new(),
                latest_engines: HashMap::new(),
                versioned_engines: HashMap::new(),
            }),
        }
    }

    pub async fn register(&self, registration: FlowRegistration) {
        let mut inner = self.inner.lock().await;
        let flow_id = registration.flow_id.clone();
        let version = registration.version;
        let versioned_key = (flow_id.clone(), version);
        inner.versioned.insert(versioned_key.clone(), registration.clone());
        inner.versioned_engines.insert(
            versioned_key,
            Arc::new(StateMachineEngine::new(registration.manifest.clone())),
        );

        let replace_latest = inner
            .latest
            .get(&flow_id)
            .is_none_or(|existing| version > existing.version);
        if replace_latest {
            inner
                .latest_engines
                .insert(
                    flow_id.clone(),
                    Arc::new(StateMachineEngine::new(registration.manifest.clone())),
                );
            inner.latest.insert(flow_id, registration);
        }
    }

    pub async fn remove_flow(&self, flow_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.latest.remove(flow_id);
        inner.latest_engines.remove(flow_id);
        inner.versioned.retain(|(candidate, _), _| candidate != flow_id);
        inner
            .versioned_engines
            .retain(|(candidate, _), _| candidate != flow_id);
    }

    pub async fn latest_registration(&self, flow_id: &str) -> Option<FlowRegistration> {
        let inner = self.inner.lock().await;
        inner.latest.get(flow_id).cloned()
    }

    pub async fn versioned_registration(
        &self,
        flow_id: &str,
        version: Uuid,
    ) -> Option<FlowRegistration> {
        let inner = self.inner.lock().await;
        inner.versioned.get(&(flow_id.to_string(), version)).cloned()
    }

    pub async fn versioned_engine(
        &self,
        flow_id: &str,
        version: Uuid,
    ) -> Option<Arc<StateMachineEngine>> {
        let inner = self.inner.lock().await;
        inner
            .versioned_engines
            .get(&(flow_id.to_string(), version))
            .cloned()
    }

    pub async fn latest_engine(&self, flow_id: &str) -> Option<Arc<StateMachineEngine>> {
        let inner = self.inner.lock().await;
        inner.latest_engines.get(flow_id).cloned()
    }

    pub async fn latest_flow_ids(&self) -> Vec<String> {
        let inner = self.inner.lock().await;
        inner.latest.keys().cloned().collect()
    }

    pub async fn counts(&self) -> (usize, usize) {
        let inner = self.inner.lock().await;
        (inner.latest.len(), inner.versioned.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiroha_core::flow::{FlowManifest, FlowWorld, StateDef, StateKind, TransitionDef};

    fn manifest(flow_id: &str, done_state: &str) -> FlowManifest {
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
                    name: done_state.to_string(),
                    kind: StateKind::Terminal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
            ],
            transitions: vec![TransitionDef {
                from: "idle".into(),
                to: done_state.to_string(),
                event: "approve".into(),
                guard: None,
                action: None,
                timeout: None,
            }],
            initial_state: "idle".into(),
            actions: vec![],
        }
    }

    fn registration(flow_id: &str, version: Uuid, done_state: &str) -> FlowRegistration {
        FlowRegistration {
            flow_id: flow_id.to_string(),
            version,
            manifest: manifest(flow_id, done_state),
            wasm_hash: format!("test-{flow_id}-{version}"),
        }
    }

    #[tokio::test]
    async fn register_version_updates_latest_when_version_is_newer() {
        let registry = FlowRegistry::new();
        let old_version = Uuid::now_v7();
        let new_version = Uuid::now_v7();

        registry
            .register(registration("approval", old_version, "done"))
            .await;
        registry
            .register(registration("approval", new_version, "rerouted"))
            .await;

        let latest = registry
            .latest_registration("approval")
            .await
            .expect("latest registration");
        assert_eq!(latest.version, new_version);
        assert_eq!(latest.manifest.transitions[0].to, "rerouted");
    }

    #[tokio::test]
    async fn remove_flow_clears_latest_and_versioned_entries() {
        let registry = FlowRegistry::new();
        let v1 = Uuid::now_v7();
        let v2 = Uuid::now_v7();
        registry.register(registration("approval", v1, "done")).await;
        registry.register(registration("approval", v2, "rerouted")).await;

        registry.remove_flow("approval").await;

        assert!(registry.latest_registration("approval").await.is_none());
        assert!(registry.versioned_registration("approval", v1).await.is_none());
        assert!(registry.versioned_registration("approval", v2).await.is_none());
        assert!(registry.versioned_engine("approval", v1).await.is_none());
        assert!(registry.versioned_engine("approval", v2).await.is_none());
    }
}
