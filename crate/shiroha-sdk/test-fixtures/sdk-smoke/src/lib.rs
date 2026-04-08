shiroha_sdk::generate_full_flow!();
use shiroha_sdk::prelude::*;

use crate::shiroha::flow::{net, store};

struct SdkSmokeFlow;

impl Guest for SdkSmokeFlow {
    fn get_manifest() -> FlowManifest {
        flow_manifest! {
            id: "sdk-smoke-demo",
            world: Full,
            states: vec![
                flow_state!("draft", Normal, on_enter: "hydrate"),
                flow_state!(
                    "legal-review",
                    Subprocess,
                    subprocess: flow_subprocess!("legal-review-demo", "legal-review-complete")
                ),
                flow_state!("done", Terminal, on_exit: "ship"),
            ],
            transitions: vec![
                flow_transition!(
                    "draft",
                    "submit",
                    "legal-review",
                    guard: "allow-submit",
                    action: "hydrate"
                ),
                flow_transition!(
                    "legal-review",
                    "legal-review-complete",
                    "done",
                    action: "ship",
                    timeout: flow_timeout!(250, "legal-review-timeout")
                ),
            ],
            initial_state: "draft",
            actions: vec![
                flow_action!("hydrate", DispatchMode::Local, caps: [Storage]),
                remote_action!("ship", caps: [Network]),
                fanout_action!(
                    "collect-quote",
                    strategy: FanOutStrategy::Tagged(vec!["edge-a".to_string(), "edge-b".to_string()]),
                    aggregator: "pick-success",
                    timeout_ms: 1_500,
                    min_success: 1,
                    caps: [Network]
                ),
                local_action!("allow-submit"),
            ],
        }
    }

    fn supports_action(name: String) -> bool {
        matches!(name.as_str(), "hydrate" | "ship" | "collect-quote")
    }

    fn supports_guard(name: String) -> bool {
        matches!(name.as_str(), "allow-submit")
    }

    fn supports_aggregate(name: String) -> bool {
        matches!(name.as_str(), "pick-success")
    }

    fn invoke_action(name: String, ctx: ActionContext) -> ActionResult {
        match name.as_str() {
            "hydrate" => {
                store::put("sdk-smoke", "job", ctx.job_id.as_bytes());
                let cached = store::get("sdk-smoke", "job")
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                    .unwrap_or_else(|| "<missing>".to_string());
                action_ok!(Some(format!("hydrated:{cached}").into_bytes()))
            }
            "ship" => {
                let request = net::RequestOptions {
                    method: net::HttpMethod::Post,
                    url: "http://127.0.0.1:1/sdk-smoke".to_string(),
                    headers: vec![net::Header {
                        name: "x-sdk".to_string(),
                        value: "smoke".to_string(),
                    }],
                    query: Vec::new(),
                    version: Some(net::HttpVersion::Http11),
                    timeout_ms: Some(250),
                    bearer_token: None,
                    basic_auth: None,
                    body: ctx.payload.clone(),
                    error_for_status: Some(false),
                };

                let request_status = match net::send(None, &request) {
                    Ok(response) => format!("status:{}", response.status),
                    Err(error) => format!("error:{:?}", error.kind),
                };
                action_ok!(Some(request_status.into_bytes()))
            }
            "collect-quote" => action_ok!(Some(b"fanout".to_vec())),
            other => action_fail!(Some(format!("unknown action: {other}").into_bytes())),
        }
    }

    fn invoke_guard(name: String, ctx: GuardContext) -> bool {
        match name.as_str() {
            "allow-submit" => ctx.event == "submit" && ctx.payload.is_some(),
            _ => false,
        }
    }

    fn aggregate(name: String, results: Vec<NodeResult>) -> AggregateDecision {
        let success_count = results
            .iter()
            .filter(|result| result.status == ExecutionStatus::Success)
            .count();

        match name.as_str() {
            "pick-success" if success_count > 0 => {
                aggregate_event!("quote-ready".to_string(), Some(format!("success_count={success_count}").into_bytes()))
            }
            "pick-success" => {
                aggregate_event!("quote-retry".to_string(), Some(b"success_count=0".to_vec()))
            }
            _ => aggregate_event!(format!("unsupported-aggregate:{name}"), None),
        }
    }
}

export!(SdkSmokeFlow);
