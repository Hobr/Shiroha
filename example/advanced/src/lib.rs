wit_bindgen::generate!({
    path: "../../crate/shiroha-wasm/wit",
    world: "flow",
});

struct AdvancedFlow;

impl Guest for AdvancedFlow {
    fn get_manifest() -> FlowManifest {
        FlowManifest {
            id: "advanced-orchestration-demo".to_string(),
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
                        flow_id: "legal-review-flow".to_string(),
                        completion_event: "legal-review-complete".to_string(),
                    }),
                },
                StateDef {
                    name: "quote-collection".to_string(),
                    kind: StateKind::Normal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: "waiting-approval".to_string(),
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
                StateDef {
                    name: "timed-out".to_string(),
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
                    guard: Some("has-minimum-payload".to_string()),
                    action: Some("normalize-request".to_string()),
                    timeout: None,
                },
                TransitionDef {
                    from: "legal-review".to_string(),
                    to: "quote-collection".to_string(),
                    event: "legal-review-complete".to_string(),
                    guard: None,
                    action: None,
                    timeout: None,
                },
                TransitionDef {
                    from: "quote-collection".to_string(),
                    to: "waiting-approval".to_string(),
                    event: "collect-quotes".to_string(),
                    guard: None,
                    action: Some("collect-quotes".to_string()),
                    timeout: None,
                },
                TransitionDef {
                    from: "waiting-approval".to_string(),
                    to: "approved".to_string(),
                    event: "approve".to_string(),
                    guard: Some("allow-approve".to_string()),
                    action: Some("ship".to_string()),
                    timeout: None,
                },
                TransitionDef {
                    from: "waiting-approval".to_string(),
                    to: "rejected".to_string(),
                    event: "reject".to_string(),
                    guard: None,
                    action: None,
                    timeout: None,
                },
                TransitionDef {
                    from: "waiting-approval".to_string(),
                    to: "timed-out".to_string(),
                    event: "expire".to_string(),
                    guard: None,
                    action: None,
                    timeout: Some(TimeoutDef {
                        duration_ms: 30_000,
                        timeout_event: "expire".to_string(),
                    }),
                },
            ],
            initial_state: "draft".to_string(),
            actions: vec![
                ActionDef {
                    name: "normalize-request".to_string(),
                    dispatch: DispatchMode::Local,
                },
                ActionDef {
                    name: "collect-quotes".to_string(),
                    dispatch: DispatchMode::FanOut(FanOutConfig {
                        strategy: FanOutStrategy::Count(3),
                        aggregator: "pick-success".to_string(),
                        timeout_ms: Some(15_000),
                        min_success: Some(1),
                    }),
                },
                ActionDef {
                    name: "ship".to_string(),
                    dispatch: DispatchMode::Remote,
                },
            ],
        }
    }

    fn invoke_action(name: String, ctx: ActionContext) -> ActionResult {
        match name.as_str() {
            "normalize-request" => ActionResult {
                status: ExecutionStatus::Success,
                output: Some(
                    format!(
                        "normalized job={} state={} payload={}",
                        ctx.job_id,
                        ctx.state,
                        ctx.payload.as_ref().map_or(0, Vec::len)
                    )
                    .into_bytes(),
                ),
            },
            "collect-quotes" => ActionResult {
                status: ExecutionStatus::Success,
                output: Some(
                    format!(
                        "quote-worker job={} state={} payload={}",
                        ctx.job_id,
                        ctx.state,
                        ctx.payload.as_ref().map_or(0, Vec::len)
                    )
                    .into_bytes(),
                ),
            },
            "ship" => ActionResult {
                status: ExecutionStatus::Success,
                output: Some(format!("remote-ship job={} state={}", ctx.job_id, ctx.state).into_bytes()),
            },
            other => ActionResult {
                status: ExecutionStatus::Failed,
                output: Some(format!("unknown action: {other}").into_bytes()),
            },
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
            "pick-success" if success_count > 0 => AggregateDecision {
                event: "quotes-collected".to_string(),
                context_patch: Some(
                    format!("success_count={success_count};nodes={successful_nodes}").into_bytes(),
                ),
            },
            "pick-success" => AggregateDecision {
                event: "quote-failed".to_string(),
                context_patch: Some(b"success_count=0".to_vec()),
            },
            _ => AggregateDecision {
                event: "fallback".to_string(),
                context_patch: None,
            },
        }
    }
}

export!(AdvancedFlow);
