shiroha_sdk::generate_flow!();

struct WarningDeadlockFlow;

impl Guest for WarningDeadlockFlow {
    fn get_manifest() -> FlowManifest {
        FlowManifest {
            id: "warning-deadlock-demo".to_string(),
            host_world: FlowWorld::Sandbox,
            states: vec![
                StateDef {
                    name: "idle".to_string(),
                    kind: StateKind::Normal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: "loop".to_string(),
                    kind: StateKind::Normal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
                StateDef {
                    name: "done".to_string(),
                    kind: StateKind::Terminal,
                    on_enter: None,
                    on_exit: None,
                    subprocess: None,
                },
            ],
            transitions: vec![
                TransitionDef {
                    from: "idle".to_string(),
                    to: "loop".to_string(),
                    event: "start".to_string(),
                    guard: None,
                    action: None,
                    timeout: None,
                },
                TransitionDef {
                    from: "loop".to_string(),
                    to: "loop".to_string(),
                    event: "spin".to_string(),
                    guard: None,
                    action: None,
                    timeout: None,
                },
            ],
            initial_state: "idle".to_string(),
            actions: vec![],
        }
    }

    fn supports_action(_name: String) -> bool {
        false
    }

    fn supports_guard(_name: String) -> bool {
        false
    }

    fn supports_aggregate(_name: String) -> bool {
        false
    }

    fn invoke_action(name: String, _ctx: ActionContext) -> ActionResult {
        ActionResult {
            status: ExecutionStatus::Failed,
            output: Some(format!("unexpected action call: {name}").into_bytes()),
        }
    }

    fn invoke_guard(_name: String, _ctx: GuardContext) -> bool {
        false
    }

    fn aggregate(_name: String, _results: Vec<NodeResult>) -> AggregateDecision {
        AggregateDecision {
            event: "noop".to_string(),
            context_patch: None,
        }
    }
}

export!(WarningDeadlockFlow);
