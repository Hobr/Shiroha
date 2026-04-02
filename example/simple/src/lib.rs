wit_bindgen::generate!({
    path: "../../crate/shiroha-wasm/wit",
    world: "flow",
});

struct ApprovalFlow;

impl Guest for ApprovalFlow {
    fn get_manifest() -> FlowManifest {
        FlowManifest {
            id: "approval-demo".to_string(),
            states: vec![
                StateDef {
                    name: "pending-approval".to_string(),
                    kind: StateKind::Normal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: "approved".to_string(),
                    kind: StateKind::Terminal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: "rejected".to_string(),
                    kind: StateKind::Terminal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
            ],
            transitions: vec![
                TransitionDef {
                    from: "pending-approval".to_string(),
                    to: "approved".to_string(),
                    event: "approve".to_string(),
                    guard: Some("allow-approve".to_string()),
                    action: Some("ship".to_string()),
                    timeout: None,
                },
                TransitionDef {
                    from: "pending-approval".to_string(),
                    to: "rejected".to_string(),
                    event: "reject".to_string(),
                    guard: None,
                    action: None,
                    timeout: None,
                },
            ],
            initial_state: "pending-approval".to_string(),
            actions: vec![ActionDef {
                name: "ship".to_string(),
                dispatch: DispatchMode::Local,
            }],
        }
    }

    fn invoke_action(name: String, ctx: ActionContext) -> ActionResult {
        match name.as_str() {
            "ship" => {
                let payload_len = ctx.payload.as_ref().map_or(0, Vec::len);
                ActionResult {
                    status: ExecutionStatus::Success,
                    output: Some(
                        format!(
                            "job={} state={} payload_bytes={payload_len}",
                            ctx.job_id, ctx.state
                        )
                        .into_bytes(),
                    ),
                }
            }
            other => ActionResult {
                status: ExecutionStatus::Failed,
                output: Some(format!("unknown action: {other}").into_bytes()),
            },
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
            "pick-success" if success_count > 0 => AggregateDecision {
                event: "done".to_string(),
                context_patch: Some(format!("success_count={success_count}").into_bytes()),
            },
            "pick-success" => AggregateDecision {
                event: "retry".to_string(),
                context_patch: Some(b"success_count=0".to_vec()),
            },
            _ => AggregateDecision {
                event: "fallback".to_string(),
                context_patch: None,
            },
        }
    }
}

export!(ApprovalFlow);
