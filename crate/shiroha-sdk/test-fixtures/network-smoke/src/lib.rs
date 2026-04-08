shiroha_sdk::generate_network_flow!();
use shiroha_sdk::prelude::*;

use crate::shiroha::flow::net;

struct NetworkSmoke;

impl Guest for NetworkSmoke {
    fn get_manifest() -> FlowManifest {
        flow_manifest! {
            id: "sdk-network-smoke-demo",
            world: Network,
            states: vec![flow_state!("idle", Normal)],
            transitions: vec![],
            initial_state: "idle",
            actions: vec![
                local_action!("fetch", caps: [Network]),
                remote_action!("fanout", caps: [Network]),
            ],
        }
    }

    fn supports_action(name: String) -> bool {
        matches!(name.as_str(), "fetch" | "fanout")
    }

    fn supports_guard(_name: String) -> bool {
        false
    }

    fn supports_aggregate(_name: String) -> bool {
        false
    }

    fn invoke_action(name: String, _ctx: ActionContext) -> ActionResult {
        match name.as_str() {
            "fetch" => {
                let request = net::RequestOptions {
                    method: net::HttpMethod::Get,
                    url: "http://127.0.0.1:1/sdk-network-smoke".to_string(),
                    headers: vec![net::Header {
                        name: "x-sdk".to_string(),
                        value: "network".to_string(),
                    }],
                    query: Vec::new(),
                    version: Some(net::HttpVersion::Http11),
                    timeout_ms: Some(100),
                    bearer_token: None,
                    basic_auth: None,
                    body: None,
                    error_for_status: Some(false),
                };

                let summary = match net::send(None, &request) {
                    Ok(response) => format!("status:{}", response.status),
                    Err(error) => format!("error:{:?}", error.kind),
                };

                action_ok!(Some(summary.into_bytes()))
            }
            "fanout" => action_ok!(Some(b"fanout".to_vec())),
            other => action_fail!(Some(format!("unknown action: {other}").into_bytes())),
        }
    }

    fn invoke_guard(_name: String, _ctx: GuardContext) -> bool {
        true
    }

    fn aggregate(name: String, _results: Vec<NodeResult>) -> AggregateDecision {
        aggregate_event!(format!("network:{name}"), None)
    }
}

export!(NetworkSmoke);
