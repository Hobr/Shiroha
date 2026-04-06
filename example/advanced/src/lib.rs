shiroha_sdk::generate_flow!();
use shiroha_sdk::prelude::*;

struct AdvancedFlow;

impl Guest for AdvancedFlow {
    fn get_manifest() -> FlowManifest {
        flow_manifest! {
            id: "advanced-orchestration-demo",
            world: Sandbox,
            states: vec![
                flow_state!("draft", Normal),
                flow_state!(
                    "legal-review",
                    Subprocess,
                    subprocess: flow_subprocess!("legal-review-flow", "legal-review-complete")
                ),
                flow_state!("quote-collection", Normal),
                flow_state!("waiting-approval", Normal),
                flow_state!("approved", Terminal),
                flow_state!("rejected", Terminal),
                flow_state!("timed-out", Terminal),
            ],
            transitions: vec![
                flow_transition!(
                    "draft",
                    "submit",
                    "legal-review",
                    guard: "has-minimum-payload",
                    action: "normalize-request"
                ),
                flow_transition!("legal-review", "legal-review-complete", "quote-collection"),
                flow_transition!(
                    "quote-collection",
                    "collect-quotes",
                    "waiting-approval",
                    action: "collect-quotes"
                ),
                flow_transition!(
                    "waiting-approval",
                    "approve",
                    "approved",
                    guard: "allow-approve",
                    action: "ship"
                ),
                flow_transition!("waiting-approval", "reject", "rejected"),
                flow_transition!(
                    "waiting-approval",
                    "expire",
                    "timed-out",
                    timeout: flow_timeout!(30_000, "expire")
                ),
            ],
            initial_state: "draft",
            actions: vec![
                local_action!("normalize-request"),
                fanout_action!(
                    "collect-quotes",
                    strategy: FanOutStrategy::Count(3),
                    aggregator: "pick-success",
                    timeout_ms: 15_000,
                    min_success: 1
                ),
                remote_action!("ship"),
                local_action!("has-minimum-payload"),
                local_action!("allow-approve"),
            ],
        }
    }

    fn invoke_action(name: String, ctx: ActionContext) -> ActionResult {
        match name.as_str() {
            "normalize-request" => action_ok!(Some(
                format!(
                    "normalized job={} state={} payload={}",
                    ctx.job_id,
                    ctx.state,
                    ctx.payload.as_ref().map_or(0, Vec::len)
                )
                .into_bytes(),
            )),
            "collect-quotes" => action_ok!(Some(
                format!(
                    "quote-worker job={} state={} payload={}",
                    ctx.job_id,
                    ctx.state,
                    ctx.payload.as_ref().map_or(0, Vec::len)
                )
                .into_bytes(),
            )),
            "ship" => action_ok!(Some(
                format!("remote-ship job={} state={}", ctx.job_id, ctx.state).into_bytes(),
            )),
            other => action_fail!(Some(format!("unknown action: {other}").into_bytes())),
        }
    }

    fn invoke_guard(name: String, ctx: GuardContext) -> bool {
        match name.as_str() {
            "has-minimum-payload" => ctx.payload.as_ref().is_some_and(|payload| !payload.is_empty()),
            "allow-approve" => ctx.event == "approve" && ctx.to_state == "approved",
            _ => false,
        }
    }

    fn aggregate(name: String, results: Vec<NodeResult>) -> AggregateDecision {
        let success_count = results
            .iter()
            .filter(|result| result.status == ExecutionStatus::Success)
            .count();
        let successful_nodes = results
            .iter()
            .filter(|result| result.status == ExecutionStatus::Success)
            .map(|result| result.node_id.as_str())
            .collect::<Vec<_>>()
            .join(",");

        match name.as_str() {
            "pick-success" if success_count > 0 => aggregate_event!(
                "quotes-collected".to_string(),
                Some(format!("success_count={success_count};nodes={successful_nodes}").into_bytes()),
            ),
            "pick-success" => {
                aggregate_event!("quote-failed".to_string(), Some(b"success_count=0".to_vec()))
            }
            _ => aggregate_event!("fallback".to_string(), None),
        }
    }
}

export!(AdvancedFlow);
