shiroha_sdk::generate_storage_flow!();
use shiroha_sdk::prelude::*;

use crate::shiroha::flow::store;

struct StorageSmoke;

impl Guest for StorageSmoke {
    fn get_manifest() -> FlowManifest {
        flow_manifest! {
            id: "sdk-storage-smoke-demo",
            world: Storage,
            states: vec![flow_state!("idle", Normal, on_enter: "write")],
            transitions: vec![],
            initial_state: "idle",
            actions: vec![local_action!("write", caps: [Storage])],
        }
    }

    fn supports_action(name: String) -> bool {
        matches!(name.as_str(), "write")
    }

    fn supports_guard(_name: String) -> bool {
        false
    }

    fn supports_aggregate(_name: String) -> bool {
        false
    }

    fn invoke_action(name: String, ctx: ActionContext) -> ActionResult {
        match name.as_str() {
            "write" => {
                store::put("sdk-storage-smoke", "job", ctx.job_id.as_bytes());
                let stored = store::get("sdk-storage-smoke", "job")
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                    .unwrap_or_else(|| "<missing>".to_string());
                action_ok!(Some(stored.into_bytes()))
            }
            other => action_fail!(Some(format!("unknown action: {other}").into_bytes())),
        }
    }

    fn invoke_guard(_name: String, _ctx: GuardContext) -> bool {
        true
    }

    fn aggregate(name: String, _results: Vec<NodeResult>) -> AggregateDecision {
        aggregate_event!(format!("storage:{name}"), None)
    }
}

export!(StorageSmoke);
