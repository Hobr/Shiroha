shiroha_sdk::generate_storage_flow!();

use crate::shiroha::flow::store;

struct StorageFlow;

impl Guest for StorageFlow {
    fn get_manifest() -> FlowManifest {
        FlowManifest {
            id: "storage-fixture".to_string(),
            host_world: FlowWorld::Storage,
            states: vec![StateDef {
                name: "idle".to_string(),
                kind: StateKind::Normal,
                on_enter: Some("store".to_string()),
                on_exit: None,
                subprocess: None,
            }],
            transitions: vec![],
            initial_state: "idle".to_string(),
            actions: vec![ActionDef {
                name: "store".to_string(),
                dispatch: DispatchMode::Local,
                capabilities: vec![ActionCapability::Storage],
            }],
        }
    }

    fn invoke_action(name: String, _ctx: ActionContext) -> ActionResult {
        if name != "store" {
            return ActionResult {
                status: ExecutionStatus::Failed,
                output: Some(format!("unexpected action: {name}").into_bytes()),
            };
        }

        store::put("fixture", "alpha", b"one");
        store::put("fixture", "beta", b"two");
        let alpha = store::get("fixture", "alpha");
        let keys = store::list_keys("fixture", None, None);
        let deleted = store::delete("fixture", "alpha");
        let alpha_after_delete = store::get("fixture", "alpha");

        ActionResult {
            status: ExecutionStatus::Success,
            output: Some(
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
            ),
        }
    }

    fn invoke_guard(_name: String, _ctx: GuardContext) -> bool {
        true
    }

    fn aggregate(_name: String, _results: Vec<NodeResult>) -> AggregateDecision {
        AggregateDecision {
            event: "noop".to_string(),
            context_patch: None,
        }
    }
}

export!(StorageFlow);
