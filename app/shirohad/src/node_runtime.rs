//! Standalone node-side execution helpers.
//!
//! `DispatchMode::Remote` 在 Phase 1 的 standalone 中仍然与 controller 同进程，
//! 但这里为它补上一条真实的 controller -> transport -> node worker 执行边界。

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use shiroha_core::error::ShirohaError;
use shiroha_core::flow::{ActionCapability, FlowRegistration};
use shiroha_core::job::ActionResult;
use shiroha_core::transport::{InProcessTransport, Response};
use shiroha_wasm::host::{ActionContext, WasmHost};
use tokio::task::JoinHandle;
use tokio::task::JoinSet;
use uuid::Uuid;

use crate::server::ShirohaState;

pub(crate) const STANDALONE_NODE_ID: &str = "standalone";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteActionRequest {
    pub flow_id: String,
    pub flow_version: Uuid,
    pub action_name: String,
    pub action_ctx: ActionContext,
    pub capabilities: Vec<ActionCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct RemoteActionResponse {
    pub result: Option<ActionResult>,
    pub error: Option<String>,
}

fn wasm_host_for_state(
    state: &Arc<ShirohaState>,
    flow: &FlowRegistration,
) -> Result<WasmHost, ShirohaError> {
    let module = state
        .module_cache
        .get(&flow.wasm_hash)
        .ok_or_else(|| ShirohaError::Wasm(format!("WASM module for flow `{}` is not available in cache; redeploy the flow in this process", flow.flow_id)))?;
    WasmHost::new_with_capability_store(
        state.wasm_runtime.engine(),
        module.component(),
        state.storage.clone(),
    )
    .map_err(|e| ShirohaError::Wasm(e.to_string()))
}

async fn execute_remote_action(
    state: Arc<ShirohaState>,
    request: RemoteActionRequest,
) -> Result<ActionResult, String> {
    let flow = state
        .flow_registry
        .versioned_registration(&request.flow_id, request.flow_version)
        .await
        .ok_or_else(|| {
            format!(
                "flow `{}` version {} not loaded in memory",
                request.flow_id, request.flow_version
            )
        })?;

    tokio::task::spawn_blocking(move || {
        let mut host = wasm_host_for_state(&state, &flow).map_err(|e| e.to_string())?;
        host.invoke_action(
            &request.action_name,
            request.action_ctx,
            &request.capabilities,
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|error| format!("node action task join error: {error}"))?
}

fn encode_response(response: RemoteActionResponse) -> Result<Response, ShirohaError> {
    let payload = serde_json::to_vec(&response).map_err(|error| {
        ShirohaError::Transport(format!("failed to encode node response: {error}"))
    })?;
    Ok(Response { payload })
}

async fn handle_transport_request(
    state: Arc<ShirohaState>,
    payload: Vec<u8>,
) -> Result<Response, ShirohaError> {
    let decoded: RemoteActionRequest = serde_json::from_slice(&payload).map_err(|error| {
        ShirohaError::Transport(format!("failed to decode node request: {error}"))
    })?;

    let response = match execute_remote_action(state, decoded).await {
        Ok(result) => RemoteActionResponse {
            result: Some(result),
            error: None,
        },
        Err(error) => RemoteActionResponse {
            result: None,
            error: Some(error),
        },
    };

    encode_response(response)
}

pub(crate) async fn spawn_standalone_node_worker(
    state: Arc<ShirohaState>,
    transport: Arc<InProcessTransport>,
) -> JoinHandle<()> {
    let mut receiver = transport.register_node(STANDALONE_NODE_ID).await;
    tokio::spawn(async move {
        let mut in_flight = JoinSet::new();

        loop {
            tokio::select! {
                Some(request) = receiver.recv() => {
                    let state = state.clone();
                    in_flight.spawn(async move {
                        let response = handle_transport_request(state, request.message.payload).await;
                        let _ = request.respond.send(response);
                    });
                }
                Some(joined) = in_flight.join_next(), if !in_flight.is_empty() => {
                    let _ = joined;
                }
                else => break,
            }
        }

        in_flight.abort_all();
        while let Some(joined) = in_flight.join_next().await {
            let _ = joined;
        }
    })
}
