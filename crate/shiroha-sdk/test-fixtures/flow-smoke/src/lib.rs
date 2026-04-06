shiroha_sdk::generate_flow!();
use shiroha_sdk::prelude::*;

struct FlowSmoke;

impl Guest for FlowSmoke {
    fn get_manifest() -> FlowManifest {
        flow_manifest! {
            id: "sdk-flow-smoke-demo",
            world: Sandbox,
            states: vec![
                flow_state!("draft", Normal, on_enter: "prepare"),
                flow_state!("done", Terminal),
            ],
            transitions: vec![flow_transition!(
                "draft",
                "submit",
                "done",
                guard: "allow-submit",
                action: "prepare",
                timeout: flow_timeout!(100, "expire")
            )],
            initial_state: "draft",
            actions: vec![
                local_action!("prepare"),
                local_action!("allow-submit"),
                fanout_action!(
                    "collect",
                    strategy: FanOutStrategy::Count(2),
                    aggregator: "pick-success",
                    min_success: 1
                ),
            ],
        }
    }

    fn invoke_action(name: String, ctx: ActionContext) -> ActionResult {
        match name.as_str() {
            "prepare" => action_ok!(Some(format!("prepared:{}", ctx.job_id).into_bytes())),
            "collect" => action_ok!(Some(b"collect".to_vec())),
            other => action_fail!(Some(format!("unknown action: {other}").into_bytes())),
        }
    }

    fn invoke_guard(name: String, ctx: GuardContext) -> bool {
        match name.as_str() {
            "allow-submit" => ctx.event == "submit",
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
            _ => aggregate_event!(format!("unsupported-aggregate:{name}"), None),
        }
    }
}

export!(FlowSmoke);
