use std::sync::Arc;

use shiroha_core::flow::{FlowManifest, FlowRegistration};
use shiroha_core::storage::Storage;
use shiroha_proto::shiroha_api::flow_service_server::FlowService;
use shiroha_proto::shiroha_api::job_service_server::JobService;
use shiroha_proto::shiroha_api::{DeployFlowRequest, GetJobRequest, GetJobResponse};
use tonic::Request;
use uuid::Uuid;

use crate::flow_service::FlowServiceImpl;
use crate::job_service::JobServiceImpl;
use crate::server::ShirohaState;
use crate::test_support::wasm_for_manifest;

/// Deploy a flow fixture through `FlowServiceImpl`.
///
/// Test fixtures should keep request `flow_id` and `manifest.id` aligned so assertions
/// about flow identity do not silently drift.
pub(crate) async fn deploy_flow(state: Arc<ShirohaState>, flow_id: &str, manifest: &FlowManifest) {
    assert_eq!(
        flow_id, manifest.id,
        "test helper invariant: deploy flow_id must match manifest.id"
    );

    let flow_service = FlowServiceImpl::new(state);
    flow_service
        .deploy_flow(Request::new(DeployFlowRequest {
            flow_id: flow_id.to_string(),
            wasm_bytes: wasm_for_manifest(manifest),
        }))
        .await
        .expect("deploy flow");
}

/// Register a specific flow version for metadata/version-binding tests.
///
/// This helper is intentionally metadata-only: it seeds storage + in-memory registry
/// with a synthetic `wasm_hash`, and does NOT build/install wasm bytes into module cache.
pub(crate) async fn register_flow_version(
    state: &Arc<ShirohaState>,
    flow_id: &str,
    version: Uuid,
    manifest: FlowManifest,
) -> FlowRegistration {
    let registration = FlowRegistration {
        flow_id: flow_id.to_string(),
        version,
        manifest: manifest.clone(),
        wasm_hash: format!("test-{flow_id}-{version}"),
    };

    state
        .storage
        .save_flow(&registration)
        .await
        .expect("save flow");
    state.flow_registry.register(registration.clone()).await;

    registration
}

pub(crate) async fn wait_for_job(
    service: &JobServiceImpl,
    job_id: &str,
    expected_state: &str,
    expected_current_state: &str,
) -> GetJobResponse {
    tokio::time::timeout(std::time::Duration::from_millis(400), async {
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
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("job should reach expected state")
}
