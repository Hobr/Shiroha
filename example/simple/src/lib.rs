shiroha_sdk::generate_flow!();
use shiroha_sdk::prelude::*;

struct ApprovalFlow;

impl Guest for ApprovalFlow {
    fn get_manifest() -> FlowManifest {
        flow_manifest! {
            id: "approval-demo",
            world: Sandbox,
            states: vec![
                flow_state!("pending-approval", Normal),
                flow_state!("approved", Terminal),
                flow_state!("rejected", Terminal),
            ],
            transitions: vec![
                flow_transition!("pending-approval", "approve", "approved", guard: "allow-approve", action: "ship"),
                flow_transition!("pending-approval", "reject", "rejected"),
            ],
            initial_state: "pending-approval",
            actions: vec![
                local_action!("ship"),
                local_action!("allow-approve"),
            ],
        }
    }

    fn invoke_action(name: String, ctx: ActionContext) -> ActionResult {
        match name.as_str() {
            "ship" => {
                let payload_len = ctx.payload.as_ref().map_or(0, Vec::len);
                action_ok!(Some(
                    format!(
                        "job={} state={} payload_bytes={payload_len}",
                        ctx.job_id, ctx.state
                    )
                    .into_bytes(),
                ))
            }
            other => action_fail!(Some(format!("unknown action: {other}").into_bytes())),
        }
    }

    fn invoke_guard(name: String, ctx: GuardContext) -> bool {
        match name.as_str() {
            "allow-approve" => ctx.event == "approve" && ctx.to_state == "approved",
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
                aggregate_event!("done".to_string(), Some(format!("success_count={success_count}").into_bytes()))
            }
            "pick-success" => aggregate_event!("retry".to_string(), Some(b"success_count=0".to_vec())),
            _ => aggregate_event!("fallback".to_string(), None),
        }
    }
}

export!(ApprovalFlow);
