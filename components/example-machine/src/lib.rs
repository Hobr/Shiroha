use shiroha_guest::{MachineGuest, export_machine, types};

struct ExampleMachine;

impl MachineGuest for ExampleMachine {
    fn get_machine() -> Result<types::MachineDefinition, types::GuestError> {
        Ok(types::MachineDefinition {
            id: "example-machine".to_owned(),
            initial: "idle".to_owned(),
            functions: vec![
                declaration("allow", types::FunctionRole::Guard),
                declaration("enter-idle", types::FunctionRole::Callback),
                declaration("exit-idle", types::FunctionRole::Callback),
                declaration("begin", types::FunctionRole::Action),
                declaration("pause", types::FunctionRole::Action),
                declaration("spin", types::FunctionRole::Action),
                declaration("allocate", types::FunctionRole::Action),
                declaration("guest-error", types::FunctionRole::Action),
                declaration("enter-processing", types::FunctionRole::Callback),
                declaration("finish", types::FunctionRole::Action),
                declaration("enter-done", types::FunctionRole::Callback),
                declaration("enter-rejected", types::FunctionRole::Callback),
            ],
            states: vec![
                types::StateDefinition {
                    id: "idle".to_owned(),
                    entry_callback: Some(reference("enter-idle")),
                    exit_callback: Some(reference("exit-idle")),
                    terminal: None,
                    transitions: vec![
                        types::TransitionDefinition {
                            trigger: types::Trigger::Event("begin".to_owned()),
                            guard: Some(reference("allow")),
                            action: Some(reference("begin")),
                            target: "processing".to_owned(),
                            failure_target: Some("rejected".to_owned()),
                        },
                        types::TransitionDefinition {
                            trigger: types::Trigger::Event("pause".to_owned()),
                            guard: None,
                            action: Some(reference("pause")),
                            target: "processing".to_owned(),
                            failure_target: None,
                        },
                        types::TransitionDefinition {
                            trigger: types::Trigger::Event("spin".to_owned()),
                            guard: None,
                            action: Some(reference("spin")),
                            target: "idle".to_owned(),
                            failure_target: None,
                        },
                        types::TransitionDefinition {
                            trigger: types::Trigger::Event("guest-error".to_owned()),
                            guard: None,
                            action: Some(reference("guest-error")),
                            target: "idle".to_owned(),
                            failure_target: None,
                        },
                        types::TransitionDefinition {
                            trigger: types::Trigger::Event("allocate".to_owned()),
                            guard: None,
                            action: Some(reference("allocate")),
                            target: "idle".to_owned(),
                            failure_target: None,
                        },
                    ],
                },
                types::StateDefinition {
                    id: "processing".to_owned(),
                    entry_callback: Some(reference("enter-processing")),
                    exit_callback: None,
                    terminal: None,
                    transitions: vec![types::TransitionDefinition {
                        trigger: types::Trigger::Event("finish".to_owned()),
                        guard: None,
                        action: Some(reference("finish")),
                        target: "done".to_owned(),
                        failure_target: None,
                    }],
                },
                types::StateDefinition {
                    id: "done".to_owned(),
                    entry_callback: Some(reference("enter-done")),
                    exit_callback: None,
                    terminal: Some(types::TerminalKind::Completed),
                    transitions: Vec::new(),
                },
                types::StateDefinition {
                    id: "rejected".to_owned(),
                    entry_callback: Some(reference("enter-rejected")),
                    exit_callback: None,
                    terminal: Some(types::TerminalKind::Failed),
                    transitions: Vec::new(),
                },
            ],
        })
    }

    fn evaluate_guard(id: String, _: types::GuardInput) -> Result<bool, types::GuestError> {
        match id.as_str() {
            "allow" => Ok(true),
            _ => Err(types::GuestError::unknown_function(&id)),
        }
    }

    fn invoke_callback(
        id: String,
        _: types::HookInput,
    ) -> Result<types::HookEffects, types::GuestError> {
        match id.as_str() {
            "enter-idle" | "exit-idle" | "enter-processing" | "enter-done" => {
                Ok(types::HookEffects::none())
            }
            "enter-rejected" => Ok(types::HookEffects {
                replacement_context: Some(types::Payload::json(
                    br#"{"phase":"rejected"}"#.to_vec(),
                )),
                events: Vec::new(),
            }),
            _ => Err(types::GuestError::unknown_function(&id)),
        }
    }

    fn invoke_action(
        id: String,
        input: types::HookInput,
    ) -> Result<types::ActionOutcome, types::GuestError> {
        match id.as_str() {
            "begin" if input.context.data == br#"{"fail":true}"# => {
                Ok(types::ActionOutcome::Failed(types::BusinessFailure {
                    code: "rejected".to_owned(),
                    payload: None,
                    effects: types::HookEffects {
                        replacement_context: Some(types::Payload::json(
                            br#"{"phase":"failed"}"#.to_vec(),
                        )),
                        events: Vec::new(),
                    },
                }))
            }
            "begin" => Ok(types::ActionOutcome::Succeeded(types::HookEffects {
                replacement_context: Some(types::Payload::json(
                    br#"{"phase":"processing"}"#.to_vec(),
                )),
                events: vec![types::Event {
                    name: "finish".to_owned(),
                    payload: None,
                }],
            })),
            "pause" => Ok(types::ActionOutcome::Succeeded(types::HookEffects {
                replacement_context: Some(types::Payload::json(
                    br#"{"phase":"processing"}"#.to_vec(),
                )),
                events: Vec::new(),
            })),
            "finish" => Ok(types::ActionOutcome::Succeeded(types::HookEffects {
                replacement_context: Some(types::Payload::json(br#"{"phase":"done"}"#.to_vec())),
                events: Vec::new(),
            })),
            "spin" => loop {
                std::hint::spin_loop();
            },
            "allocate" => {
                let mut allocation = vec![0_u8; 32 * 1024 * 1024];
                std::hint::black_box(&mut allocation);
                Ok(types::ActionOutcome::Succeeded(types::HookEffects::none()))
            }
            "guest-error" => Err(types::GuestError {
                code: "example-error".to_owned(),
                message: "guest rejected the invocation".to_owned(),
                payload: Some(types::Payload::json(br#"{"detail":true}"#.to_vec())),
            }),
            _ => Err(types::GuestError::unknown_function(&id)),
        }
    }
}

fn reference(locator: &str) -> types::FunctionRef {
    types::FunctionRef {
        kind: "wasm-component".to_owned(),
        locator: locator.to_owned(),
    }
}

fn declaration(locator: &str, role: types::FunctionRole) -> types::FunctionDeclaration {
    types::FunctionDeclaration {
        reference: reference(locator),
        role,
    }
}

export_machine!(ExampleMachine);
