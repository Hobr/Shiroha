//! WASM adapter implementing the Adapter trait.

use async_trait::async_trait;
use shiroha_engine::Adapter;
use shiroha_ir::{
    ActionRef, EventDef, GuardRef, HistoryConfig, State, StateMachineDef, Transition,
};
use std::path::Path;
use std::sync::Arc;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use crate::bindings::StateMachine;
use crate::host::HostImpl;
use crate::{Result, WasmError};

/// Store state for adapter operations.
struct AdapterStoreState {
    wasi: WasiCtx,
    table: ResourceTable,
    host: HostImpl,
}

impl WasiView for AdapterStoreState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

/// WASM Component Model adapter for loading state machine definitions.
pub struct WasmAdapter {
    engine: Arc<Engine>,
    component: Component,
}

impl WasmAdapter {
    /// Create a new WASM adapter from a component file.
    pub fn from_file(engine: Arc<Engine>, path: impl AsRef<Path>) -> Result<Self> {
        let component = Component::from_file(&engine, path.as_ref()).map_err(|e| {
            WasmError::ComponentLoad(format!(
                "Failed to load component from {}: {}",
                path.as_ref().display(),
                e
            ))
        })?;

        Ok(Self { engine, component })
    }

    /// Create a new WASM adapter from component bytes.
    pub fn from_bytes(engine: Arc<Engine>, bytes: &[u8]) -> Result<Self> {
        let component = Component::from_binary(&engine, bytes)
            .map_err(|e| WasmError::ComponentLoad(format!("Failed to load component: {}", e)))?;

        Ok(Self { engine, component })
    }

    /// Create a store for adapter operations.
    fn create_store(&self) -> Result<Store<AdapterStoreState>> {
        let wasi = WasiCtxBuilder::new().inherit_stdio().build();
        let table = ResourceTable::new();
        let state = AdapterStoreState {
            wasi,
            table,
            host: HostImpl,
        };

        Ok(Store::new(&self.engine, state))
    }

    /// Instantiate the component.
    fn instantiate(&self, store: &mut Store<AdapterStoreState>) -> Result<StateMachine> {
        let mut linker = Linker::new(&self.engine);

        // Link WASI
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
            .map_err(|e| WasmError::Link(format!("Failed to link WASI: {}", e)))?;

        // Link host imports
        StateMachine::add_to_linker::<AdapterStoreState, wasmtime::component::HasSelf<HostImpl>>(
            &mut linker,
            |state: &mut AdapterStoreState| &mut state.host,
        )
        .map_err(|e| WasmError::Link(format!("Failed to link host imports: {}", e)))?;

        // Instantiate component
        let bindings = StateMachine::instantiate(store, &self.component, &linker)
            .map_err(|e| WasmError::Instantiation(format!("Failed to instantiate: {}", e)))?;

        Ok(bindings)
    }

    /// Convert WIT ActionRef to IR ActionRef.
    fn convert_action_ref(
        action: &crate::bindings::shiroha::sm::types::ActionRef,
    ) -> anyhow::Result<ActionRef> {
        use crate::bindings::shiroha::sm::types::ActionKind;
        use shiroha_ir::ActionKind as IrActionKind;

        let kind = match &action.kind {
            ActionKind::Wasm(name) => IrActionKind::Wasm(name.clone()),
            ActionKind::Plugin(name) => IrActionKind::Plugin(name.clone()),
        };

        Ok(ActionRef {
            name: action.name.clone(),
            kind,
        })
    }

    /// Convert WIT GuardRef to IR GuardRef.
    fn convert_guard_ref(guard: &crate::bindings::shiroha::sm::types::GuardKind) -> GuardRef {
        use crate::bindings::shiroha::sm::types::GuardKind;

        match guard {
            GuardKind::Always => GuardRef::Always,
            GuardKind::Wasm(name) => GuardRef::Wasm(name.clone()),
            GuardKind::Plugin(name) => GuardRef::Plugin(name.clone()),
        }
    }

    /// Convert WIT HistoryKind to IR HistoryConfig.
    fn convert_history(
        history: &crate::bindings::shiroha::sm::types::HistoryKind,
    ) -> HistoryConfig {
        use crate::bindings::shiroha::sm::types::HistoryKind;

        match history {
            HistoryKind::None => HistoryConfig::None,
            HistoryKind::Shallow => HistoryConfig::Shallow,
            HistoryKind::Deep => HistoryConfig::Deep,
        }
    }
}

#[async_trait]
impl Adapter for WasmAdapter {
    async fn load(&self) -> anyhow::Result<StateMachineDef> {
        // Create store and instantiate component
        let mut store = self
            .create_store()
            .map_err(|e| anyhow::anyhow!("Failed to create store: {}", e))?;

        let bindings = self
            .instantiate(&mut store)
            .map_err(|e| anyhow::anyhow!("Failed to instantiate component: {}", e))?;

        // Query definition interface
        let def_interface = bindings.shiroha_sm_definition();

        // Get initial state
        let initial = def_interface
            .call_initial(&mut store)
            .map_err(|e| anyhow::anyhow!("Failed to get initial state: {}", e))?;

        // Get states
        let wit_states = def_interface
            .call_states(&mut store)
            .map_err(|e| anyhow::anyhow!("Failed to get states: {}", e))?;

        let states: std::result::Result<Vec<State>, anyhow::Error> = wit_states
            .into_iter()
            .map(|s| {
                Ok(State {
                    id: s.name,
                    parent: s.parent,
                    entry: s.entry.as_ref().map(Self::convert_action_ref).transpose()?,
                    exit: s.exit.as_ref().map(Self::convert_action_ref).transpose()?,
                    do_activity: s
                        .do_activity
                        .as_ref()
                        .map(Self::convert_action_ref)
                        .transpose()?,
                    history: Self::convert_history(&s.history),
                    ortho: None, // MVP: no orthogonal regions
                })
            })
            .collect();

        let states = states?;

        // Get transitions
        let wit_transitions = def_interface
            .call_transitions(&mut store)
            .map_err(|e| anyhow::anyhow!("Failed to get transitions: {}", e))?;

        let transitions: std::result::Result<Vec<Transition>, anyhow::Error> = wit_transitions
            .into_iter()
            .map(|t| {
                Ok(Transition {
                    from: t.from,
                    to: t.to,
                    event: t.event,
                    guard: t.guard.as_ref().map(Self::convert_guard_ref),
                    action: t
                        .action
                        .as_ref()
                        .map(Self::convert_action_ref)
                        .transpose()?,
                })
            })
            .collect();

        let transitions = transitions?;

        // Get events
        let wit_events = def_interface
            .call_events(&mut store)
            .map_err(|e| anyhow::anyhow!("Failed to get events: {}", e))?;

        let events: Vec<EventDef> = wit_events
            .into_iter()
            .map(|e| EventDef { name: e.name })
            .collect();

        // Construct IR
        Ok(StateMachineDef {
            name: "wasm-sm".to_string(), // TODO: get from component metadata
            initial,
            states,
            transitions,
            events,
        })
    }
}
