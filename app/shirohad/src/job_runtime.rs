pub(crate) struct ScheduledAction<'a> {
    pub(crate) action_name: &'a str,
    pub(crate) action_state: &'a str,
}

pub(crate) fn action_sequence<'a>(
    from: &'a str,
    to: &'a str,
    on_exit: Option<&'a str>,
    transition_action: Option<&'a str>,
    on_enter: Option<&'a str>,
) -> Vec<ScheduledAction<'a>> {
    let mut sequence = Vec::new();
    if let Some(action_name) = on_exit {
        sequence.push(ScheduledAction {
            action_name,
            action_state: from,
        });
    }
    if let Some(action_name) = transition_action {
        sequence.push(ScheduledAction {
            action_name,
            action_state: to,
        });
    }
    if let Some(action_name) = on_enter {
        sequence.push(ScheduledAction {
            action_name,
            action_state: to,
        });
    }

    sequence
}

#[cfg(test)]
mod tests {
    use super::{ScheduledAction, action_sequence};

    #[test]
    fn action_sequence_orders_exit_transition_and_enter_hooks() {
        let sequence = action_sequence(
            "pending",
            "running",
            Some("exit_pending"),
            Some("run_transition"),
            Some("enter_running"),
        );

        let expected = vec![
            ScheduledAction {
                action_name: "exit_pending",
                action_state: "pending",
            },
            ScheduledAction {
                action_name: "run_transition",
                action_state: "running",
            },
            ScheduledAction {
                action_name: "enter_running",
                action_state: "running",
            },
        ];

        assert_eq!(
            sequence
                .iter()
                .map(|it| (it.action_name, it.action_state))
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|it| (it.action_name, it.action_state))
                .collect::<Vec<_>>()
        );
    }
}
