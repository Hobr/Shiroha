shiroha_sdk::generate_network_flow!();
use shiroha_sdk::prelude::*;

use crate::shiroha::flow::net;

struct NetworkFlow;

impl Guest for NetworkFlow {
    fn get_manifest() -> FlowManifest {
        flow_manifest! {
            id: "network-fixture",
            world: Network,
            states: vec![flow_state!("idle", Normal)],
            transitions: vec![],
            initial_state: "idle",
            actions: vec![local_action!("fetch", caps: [Network])],
        }
    }

    fn supports_action(name: String) -> bool {
        matches!(name.as_str(), "fetch")
    }

    fn supports_guard(_name: String) -> bool {
        false
    }

    fn supports_aggregate(_name: String) -> bool {
        false
    }

    fn invoke_action(name: String, _ctx: ActionContext) -> ActionResult {
        if name != "fetch" {
            return action_fail!(Some(format!("unexpected action: {name}").into_bytes()));
        }

        let client = net::ClientConfig {
                default_headers: vec![net::Header {
                    name: "x-default".to_string(),
                    value: "fixture".to_string(),
                }],
                user_agent: Some("shiroha-network-fixture/1.0".to_string()),
                timeout_ms: Some(5_000),
                connect_timeout_ms: Some(5_000),
                pool_idle_timeout_ms: None,
                pool_max_idle_per_host: None,
                tcp_keepalive_ms: None,
                tcp_nodelay: Some(true),
                referer: Some(true),
                gzip: Some(true),
                brotli: Some(true),
                zstd: Some(true),
                deflate: Some(true),
                cookie_store: Some(false),
                no_proxy: Some(true),
                http1_only: Some(true),
                http2_prior_knowledge: Some(false),
                redirect_policy: Some(net::RedirectPolicy::None),
                proxies: Vec::new(),
                tls: None,
                local_address: None,
            };
        let request = net::RequestOptions {
                method: net::HttpMethod::Get,
                url: env!("SHIROHA_NETWORK_URL").to_string(),
                headers: vec![net::Header {
                    name: "x-request".to_string(),
                    value: "network".to_string(),
                }],
                query: vec![net::Header {
                    name: "lang".to_string(),
                    value: "rust".to_string(),
                }],
                version: Some(net::HttpVersion::Http11),
                timeout_ms: Some(2_000),
                bearer_token: Some("secret-token".to_string()),
                basic_auth: None,
                body: None,
                error_for_status: Some(true),
            };
        let response = net::send(Some(&client), &request);

        match response {
            Ok(response) => action_ok!(Some(
                format!(
                    "status={} version={:?} body={}",
                    response.status,
                    response.version,
                    String::from_utf8_lossy(&response.body)
                )
                .into_bytes(),
            )),
            Err(error) => action_fail!(Some(format!("network error: {}", error.message).into_bytes())),
        }
    }

    fn invoke_guard(_name: String, _ctx: GuardContext) -> bool {
        true
    }

    fn aggregate(_name: String, _results: Vec<NodeResult>) -> AggregateDecision {
        aggregate_event!("noop".to_string(), None)
    }
}

export!(NetworkFlow);
