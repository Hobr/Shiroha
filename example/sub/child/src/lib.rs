wit_bindgen::generate!({
    path: "../../../crate/shiroha-wasm/wit",
    world: "flow",
});

struct ChildFlow;

impl Guest for ChildFlow {
    fn get_manifest() -> FlowManifest {
        FlowManifest {
            id: "legal-review-demo".to_string(),
            host_world: FlowWorld::Sandbox,
            states: vec![
                StateDef {
                    name: "review-pending".to_string(),
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
                    from: "review-pending".to_string(),
                    to: "approved".to_string(),
                    event: "approve".to_string(),
                    guard: Some("allow-approval".to_string()),
                    action: Some("record-approval".to_string()),
                    timeout: None,
                },
                TransitionDef {
                    from: "review-pending".to_string(),
                    to: "rejected".to_string(),
                    event: "reject".to_string(),
                    guard: None,
                    action: Some("record-rejection".to_string()),
                    timeout: None,
                },
            ],
            initial_state: "review-pending".to_string(),
            actions: vec![
                ActionDef {
                    name: "record-approval".to_string(),
                    dispatch: DispatchMode::Local,
                },
                ActionDef {
                    name: "record-rejection".to_string(),
                    dispatch: DispatchMode::Local,
                },
                ActionDef {
                    name: "allow-approval".to_string(),
                    dispatch: DispatchMode::Local,
                },
            ],
        }
    }

    fn invoke_action(name: String, ctx: ActionContext) -> ActionResult {
        let summary = match name.as_str() {
            "record-approval" => format!("child approved job={} state={}", ctx.job_id, ctx.state),
            "record-rejection" => format!("child rejected job={} state={}", ctx.job_id, ctx.state),
            other => return ActionResult {
                status: ExecutionStatus::Failed,
                output: Some(format!("unknown action: {other}").into_bytes()),
            },
        };

        ActionResult {
            status: ExecutionStatus::Success,
            output: Some(summary.into_bytes()),
        }
    }

    fn invoke_guard(name: String, ctx: GuardContext) -> bool {
        match name.as_str() {
            "allow-approval" => ctx.event == "approve" && ctx.to_state == "approved",
            _ => false,
        }
    }

    fn aggregate(name: String, _results: Vec<NodeResult>) -> AggregateDecision {
        AggregateDecision {
            event: format!("unsupported-aggregate:{name}"),
            context_patch: None,
        }
    }
}

export!(ChildFlow);
