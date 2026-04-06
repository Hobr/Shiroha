shiroha_sdk::generate_flow!();
use shiroha_sdk::prelude::*;

struct ParentFlow;

impl Guest for ParentFlow {
    fn get_manifest() -> FlowManifest {
        flow_manifest! {
            id: "purchase-parent-demo",
            world: Sandbox,
            states: vec![
                flow_state!("draft", Normal),
                flow_state!(
                    "legal-review",
                    Subprocess,
                    subprocess: flow_subprocess!("legal-review-demo", "legal-review-complete")
                ),
                flow_state!("approved", Terminal),
                flow_state!("rejected", Terminal),
            ],
            transitions: vec![
                flow_transition!(
                    "draft",
                    "submit",
                    "legal-review",
                    guard: "has-payload",
                    action: "prepare-parent-context"
                ),
                flow_transition!(
                    "legal-review",
                    "legal-review-complete",
                    "approved",
                    action: "finalize-parent"
                ),
                flow_transition!("legal-review", "legal-review-rejected", "rejected"),
            ],
            initial_state: "draft",
            actions: vec![
                local_action!("prepare-parent-context"),
                local_action!("finalize-parent"),
                local_action!("has-payload"),
            ],
        }
    }

    fn invoke_action(name: String, ctx: ActionContext) -> ActionResult {
        match name.as_str() {
            "prepare-parent-context" => action_ok!(Some(
                format!(
                    "prepared parent job={} state={} payload={}",
                    ctx.job_id,
                    ctx.state,
                    ctx.payload.as_ref().map_or(0, Vec::len)
                )
                .into_bytes(),
            )),
            "finalize-parent" => action_ok!(Some(
                format!("finalized parent job={} state={}", ctx.job_id, ctx.state).into_bytes(),
            )),
            other => action_fail!(Some(format!("unknown action: {other}").into_bytes())),
        }
    }

    fn invoke_guard(name: String, ctx: GuardContext) -> bool {
        match name.as_str() {
            "has-payload" => ctx.payload.as_ref().is_some_and(|payload| !payload.is_empty()),
            _ => false,
        }
    }

    fn aggregate(name: String, _results: Vec<NodeResult>) -> AggregateDecision {
        aggregate_event!(format!("unsupported-aggregate:{name}"), None)
    }
}

export!(ParentFlow);
