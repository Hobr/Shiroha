use shiroha_core::flow::{
    ActionDef, DispatchMode, FlowManifest, FlowWorld, StateDef, StateKind, TimeoutDef,
    TransitionDef,
};

pub(crate) fn approval_manifest(flow_id: &str, guard: Option<&str>) -> FlowManifest {
    FlowManifest {
        id: flow_id.to_string(),
        host_world: FlowWorld::Sandbox,
        states: vec![
            StateDef {
                name: "idle".into(),
                kind: StateKind::Normal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
            StateDef {
                name: "done".into(),
                kind: StateKind::Terminal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
        ],
        transitions: vec![TransitionDef {
            from: "idle".into(),
            to: "done".into(),
            event: "approve".into(),
            guard: guard.map(str::to_string),
            action: Some("ship".into()),
            timeout: None,
        }],
        initial_state: "idle".into(),
        actions: vec![
            ActionDef {
                name: "ship".into(),
                dispatch: DispatchMode::Local,
                capabilities: Vec::new(),
            },
            ActionDef {
                name: "allow".into(),
                dispatch: DispatchMode::Local,
                capabilities: Vec::new(),
            },
            ActionDef {
                name: "deny".into(),
                dispatch: DispatchMode::Local,
                capabilities: Vec::new(),
            },
        ],
    }
}

pub(crate) fn remote_approval_manifest(flow_id: &str, guard: Option<&str>) -> FlowManifest {
    let mut manifest = approval_manifest(flow_id, guard);
    if let Some(action) = manifest
        .actions
        .iter_mut()
        .find(|action| action.name == "ship")
    {
        action.dispatch = DispatchMode::Remote;
    }
    manifest
}

pub(crate) fn approval_manifest_to(flow_id: &str, terminal_state: &str) -> FlowManifest {
    FlowManifest {
        id: flow_id.to_string(),
        host_world: FlowWorld::Sandbox,
        states: vec![
            StateDef {
                name: "idle".into(),
                kind: StateKind::Normal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
            StateDef {
                name: terminal_state.into(),
                kind: StateKind::Terminal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
        ],
        transitions: vec![TransitionDef {
            from: "idle".into(),
            to: terminal_state.into(),
            event: "approve".into(),
            guard: None,
            action: Some("ship".into()),
            timeout: None,
        }],
        initial_state: "idle".into(),
        actions: vec![ActionDef {
            name: "ship".into(),
            dispatch: DispatchMode::Local,
            capabilities: Vec::new(),
        }],
    }
}

pub(crate) fn timeout_manifest(flow_id: &str) -> FlowManifest {
    FlowManifest {
        id: flow_id.to_string(),
        host_world: FlowWorld::Sandbox,
        states: vec![
            StateDef {
                name: "waiting".into(),
                kind: StateKind::Normal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
            StateDef {
                name: "timed_out".into(),
                kind: StateKind::Terminal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
        ],
        transitions: vec![TransitionDef {
            from: "waiting".into(),
            to: "timed_out".into(),
            event: "expire".into(),
            guard: None,
            action: None,
            timeout: Some(TimeoutDef {
                duration_ms: 25,
                timeout_event: "expire".into(),
            }),
        }],
        initial_state: "waiting".into(),
        actions: Vec::new(),
    }
}

pub(crate) fn warning_manifest() -> FlowManifest {
    FlowManifest {
        id: "warning-demo".into(),
        host_world: FlowWorld::Sandbox,
        states: vec![
            StateDef {
                name: "idle".into(),
                kind: StateKind::Normal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
            StateDef {
                name: "loop".into(),
                kind: StateKind::Normal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
            StateDef {
                name: "done".into(),
                kind: StateKind::Terminal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
        ],
        transitions: vec![
            TransitionDef {
                from: "idle".into(),
                to: "loop".into(),
                event: "start".into(),
                guard: None,
                action: None,
                timeout: None,
            },
            TransitionDef {
                from: "loop".into(),
                to: "loop".into(),
                event: "spin".into(),
                guard: None,
                action: None,
                timeout: None,
            },
        ],
        initial_state: "idle".into(),
        actions: Vec::new(),
    }
}
