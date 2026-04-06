wit_bindgen::generate!({
    path: "../../../crate/shiroha-wasm/wit",
    world: "flow",
});

struct ParentFlow;

impl Guest for ParentFlow {
    fn get_manifest() -> FlowManifest {
        FlowManifest {
            id: "purchase-parent-demo".to_string(),
            host_world: FlowWorld::Sandbox,
            states: vec![
                StateDef {
                    name: "draft".to_string(),
                    kind: StateKind::Normal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: "legal-review".to_string(),
                    kind: StateKind::Subprocess,
                    on_enter: None,
                    on_exit: None,
                    subprocess: Some(SubprocessDef {
                        flow_id: "legal-review-demo".to_string(),
                        completion_event: "legal-review-complete".to_string(),
                    }),
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
                    from: "draft".to_string(),
                    to: "legal-review".to_string(),
                    event: "submit".to_string(),
                    guard: Some("has-payload".to_string()),
                    action: Some("prepare-parent-context".to_string()),
                    timeout: None,
                },
                TransitionDef {
                    from: "legal-review".to_string(),
                    to: "approved".to_string(),
                    event: "legal-review-complete".to_string(),
                    guard: None,
                    action: Some("finalize-parent".to_string()),
                    timeout: None,
                },
                TransitionDef {
                    from: "legal-review".to_string(),
                    to: "rejected".to_string(),
                    event: "legal-review-rejected".to_string(),
                    guard: None,
                    action: None,
                    timeout: None,
                },
            ],
            initial_state: "draft".to_string(),
            actions: vec![
                ActionDef {
                    name: "prepare-parent-context".to_string(),
                    dispatch: DispatchMode::Local,
                    capabilities: Vec::new(),
                },
                ActionDef {
                    name: "finalize-parent".to_string(),
                    dispatch: DispatchMode::Local,
                    capabilities: Vec::new(),
                },
                ActionDef {
                    name: "has-payload".to_string(),
                    dispatch: DispatchMode::Local,
                    capabilities: Vec::new(),
                },
            ],
        }
    }

    fn invoke_action(name: String, ctx: ActionContext) -> ActionResult {
        match name.as_str() {
            "prepare-parent-context" => ActionResult {
                status: ExecutionStatus::Success,
                output: Some(
                    format!(
                        "prepared parent job={} state={} payload={}",
                        ctx.job_id,
                        ctx.state,
                        ctx.payload.as_ref().map_or(0, Vec::len)
                    )
                    .into_bytes(),
                ),
            },
            "finalize-parent" => ActionResult {
                status: ExecutionStatus::Success,
                output: Some(format!("finalized parent job={} state={}", ctx.job_id, ctx.state).into_bytes()),
            },
            other => ActionResult {
                status: ExecutionStatus::Failed,
                output: Some(format!("unknown action: {other}").into_bytes()),
            },
        }
    }

    fn invoke_guard(name: String, ctx: GuardContext) -> bool {
        match name.as_str() {
            "has-payload" => ctx.payload.as_ref().is_some_and(|payload| !payload.is_empty()),
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

export!(ParentFlow);
