//! WASM action invoker implementing ActionInvoker trait.

use async_trait::async_trait;
use shiroha_engine::{ActionContext, ActionInvoker, ActionResult};
use std::path::Path;
use std::sync::Arc;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use crate::bindings::StateMachine;
use crate::bindings::shiroha::sm::action_types::{
    ActionContext as WitActionContext, ActionResult as WitActionResult,
};
use crate::host::HostImpl;
use crate::{Result, WasmError};

/// Store state containing both WASM component state and host imports.
struct StoreState {
    wasi: WasiCtx,
    table: ResourceTable,
    host: HostImpl,
}

impl WasiView for StoreState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

/// WASM action invoker for executing actions defined in WASM components.
pub struct WasmActionInvoker {
    engine: Arc<Engine>,
    component: Component,
}

impl WasmActionInvoker {
    /// Create a new WASM action invoker from a component file.
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

    /// Create a new WASM action invoker from component bytes.
    pub fn from_bytes(engine: Arc<Engine>, bytes: &[u8]) -> Result<Self> {
        let component = Component::from_binary(&engine, bytes)
            .map_err(|e| WasmError::ComponentLoad(format!("Failed to load component: {}", e)))?;

        Ok(Self { engine, component })
    }

    /// Create a new store with host imports linked.
    fn create_store(&self) -> Result<Store<StoreState>> {
        let wasi = WasiCtxBuilder::new().inherit_stdio().build();
        let table = ResourceTable::new();
        let state = StoreState {
            wasi,
            table,
            host: HostImpl,
        };

        Ok(Store::new(&self.engine, state))
    }

    /// Instantiate the component and link host imports.
    fn instantiate(&self, store: &mut Store<StoreState>) -> Result<StateMachine> {
        let mut linker = Linker::new(&self.engine);

        // Link WASI (component model p2)
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
            .map_err(|e| WasmError::Link(format!("Failed to link WASI: {}", e)))?;

        // Link host imports
        crate::bindings::StateMachine::add_to_linker::<
            StoreState,
            wasmtime::component::HasSelf<HostImpl>,
        >(&mut linker, |state: &mut StoreState| &mut state.host)
        .map_err(|e| WasmError::Link(format!("Failed to link host imports: {}", e)))?;

        // Instantiate component
        let bindings = StateMachine::instantiate(store, &self.component, &linker)
            .map_err(|e| WasmError::Instantiation(format!("Failed to instantiate: {}", e)))?;

        Ok(bindings)
    }

    /// Convert engine ActionContext to WIT ActionContext.
    fn convert_context(ctx: &ActionContext) -> WitActionContext {
        WitActionContext {
            task_id: ctx.task_id.clone(),
            event: ctx.event.clone(),
            payload: ctx.payload.clone(),
        }
    }

    /// Convert WIT ActionResult to engine ActionResult.
    fn convert_result(result: std::result::Result<WitActionResult, String>) -> ActionResult {
        match result {
            Ok(WitActionResult::Ok) => ActionResult::Ok,
            Ok(WitActionResult::OkValue(value)) => ActionResult::OkValue(value),
            Ok(WitActionResult::Error(msg)) => ActionResult::Error(msg),
            Ok(WitActionResult::Signal(sig)) => ActionResult::Signal(sig),
            Err(e) => ActionResult::Error(e),
        }
    }

    /// Execute an action invocation.
    fn invoke_action(&self, ctx: ActionContext, is_do: bool) -> anyhow::Result<ActionResult> {
        let mut store = self
            .create_store()
            .map_err(|e| anyhow::anyhow!("Failed to create store: {}", e))?;

        let bindings = self
            .instantiate(&mut store)
            .map_err(|e| anyhow::anyhow!("Failed to instantiate component: {}", e))?;

        let wit_ctx = Self::convert_context(&ctx);

        let result = if is_do {
            bindings
                .shiroha_sm_actions()
                .call_invoke_do(&mut store, &wit_ctx)
                .map_err(|e| anyhow::anyhow!("Failed to invoke do-activity: {}", e))?
        } else {
            bindings
                .shiroha_sm_actions()
                .call_invoke(&mut store, &wit_ctx)
                .map_err(|e| anyhow::anyhow!("Failed to invoke action: {}", e))?
        };

        Ok(Self::convert_result(result))
    }
}

#[async_trait]
impl ActionInvoker for WasmActionInvoker {
    async fn invoke_sync(&self, _name: &str, ctx: ActionContext) -> anyhow::Result<ActionResult> {
        // Run synchronously but wrapped in async for trait compatibility
        tokio::task::spawn_blocking({
            let invoker = Self {
                engine: self.engine.clone(),
                component: self.component.clone(),
            };
            move || invoker.invoke_action(ctx, false)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }

    async fn invoke_do(&self, _name: &str, ctx: ActionContext) -> anyhow::Result<ActionResult> {
        // Note: Component Model async futures are not yet stable in wasmtime 46.x
        // For MVP, we treat do-activities as synchronous calls wrapped in tokio tasks
        // The cancellation will be handled by the task manager via tokio::spawn
        tokio::task::spawn_blocking({
            let invoker = Self {
                engine: self.engine.clone(),
                component: self.component.clone(),
            };
            move || invoker.invoke_action(ctx, true)
        })
        .await
        .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?
    }
}
