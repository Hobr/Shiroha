shiroha_sdk::generate_flow!();
use shiroha_sdk::prelude::*;

struct ChildFlow;

impl Guest for ChildFlow {
    fn get_manifest() -> FlowManifest {
        flow_manifest! {
            id: "legal-review-demo",
            world: Sandbox,
            states: vec![
                flow_state!("review-pending", Normal),
                flow_state!("approved", Terminal),
                flow_state!("rejected", Terminal),
            ],
            transitions: vec![
                flow_transition!(
                    "review-pending",
                    "approve",
                    "approved",
                    guard: "allow-approval",
                    action: "record-approval"
                ),
                flow_transition!(
                    "review-pending",
                    "reject",
                    "rejected",
                    action: "record-rejection"
                ),
            ],
            initial_state: "review-pending",
            actions: vec![
                local_action!("record-approval"),
                local_action!("record-rejection"),
                local_action!("allow-approval"),
            ],
        }
    }

    fn supports_action(name: String) -> bool {
        matches!(name.as_str(), "record-approval" | "record-rejection")
    }

    fn supports_guard(name: String) -> bool {
        matches!(name.as_str(), "allow-approval")
    }

    fn supports_aggregate(_name: String) -> bool {
        false
    }

    fn invoke_action(name: String, ctx: ActionContext) -> ActionResult {
        let summary = match name.as_str() {
            "record-approval" => format!("child approved job={} state={}", ctx.job_id, ctx.state),
            "record-rejection" => format!("child rejected job={} state={}", ctx.job_id, ctx.state),
            other => return action_fail!(Some(format!("unknown action: {other}").into_bytes())),
        };

        action_ok!(Some(summary.into_bytes()))
    }

    fn invoke_guard(name: String, ctx: GuardContext) -> bool {
        match name.as_str() {
            "allow-approval" => ctx.event == "approve" && ctx.to_state == "approved",
            _ => false,
        }
    }

    fn aggregate(name: String, _results: Vec<NodeResult>) -> AggregateDecision {
        aggregate_event!(format!("unsupported-aggregate:{name}"), None)
    }
}

export!(ChildFlow);
