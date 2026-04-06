shiroha_sdk::generate_storage_flow!();
use shiroha_sdk::prelude::*;

use crate::shiroha::flow::store;

struct StorageFlow;

impl Guest for StorageFlow {
    fn get_manifest() -> FlowManifest {
        flow_manifest! {
            id: "storage-fixture",
            world: Storage,
            states: vec![flow_state!("idle", Normal, on_enter: "store")],
            transitions: vec![],
            initial_state: "idle",
            actions: vec![local_action!("store", caps: [Storage])],
        }
    }

    fn invoke_action(name: String, _ctx: ActionContext) -> ActionResult {
        if name != "store" {
            return action_fail!(Some(format!("unexpected action: {name}").into_bytes()));
        }

        store::put("fixture", "alpha", b"one");
        store::put("fixture", "beta", b"two");
        let alpha = store::get("fixture", "alpha");
        let keys = store::list_keys("fixture", None, None);
        let deleted = store::delete("fixture", "alpha");
        let alpha_after_delete = store::get("fixture", "alpha");

        action_ok!(Some(
            format!(
                "alpha={} keys={:?} deleted={} alpha_after_delete={}",
                alpha
                    .as_ref()
                    .map(|value| String::from_utf8_lossy(value).into_owned())
                    .unwrap_or_else(|| "<missing>".to_string()),
                keys,
                deleted,
                alpha_after_delete.is_some()
            )
            .into_bytes(),
        ))
    }

    fn invoke_guard(_name: String, _ctx: GuardContext) -> bool {
        true
    }

    fn aggregate(_name: String, _results: Vec<NodeResult>) -> AggregateDecision {
        aggregate_event!("noop".to_string(), None)
    }
}

export!(StorageFlow);
