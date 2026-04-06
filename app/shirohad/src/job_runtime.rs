//! Job runtime helper module.
//!
//! This module currently contains only action sequencing helpers.
//! It does not contain the full runtime coordinator yet.

pub(crate) struct ScheduledAction<'a> {
    action_name: &'a str,
    action_state: &'a str,
}

impl<'a> ScheduledAction<'a> {
    pub(crate) fn action_name(&self) -> &'a str {
        self.action_name
    }

    pub(crate) fn action_state(&self) -> &'a str {
        self.action_state
    }
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
                .map(|it| (it.action_name(), it.action_state()))
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|it| (it.action_name(), it.action_state()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn action_sequence_skips_missing_optional_actions() {
        let only_exit = action_sequence("pending", "running", Some("exit_pending"), None, None);
        assert_eq!(
            only_exit
                .iter()
                .map(|it| (it.action_name(), it.action_state()))
                .collect::<Vec<_>>(),
            vec![("exit_pending", "pending")]
        );

        let only_transition = action_sequence("pending", "running", None, Some("run_transition"), None);
        assert_eq!(
            only_transition
                .iter()
                .map(|it| (it.action_name(), it.action_state()))
                .collect::<Vec<_>>(),
            vec![("run_transition", "running")]
        );

        let only_enter = action_sequence("pending", "running", None, None, Some("enter_running"));
        assert_eq!(
            only_enter
                .iter()
                .map(|it| (it.action_name(), it.action_state()))
                .collect::<Vec<_>>(),
            vec![("enter_running", "running")]
        );

        let none = action_sequence("pending", "running", None, None, None);
        assert!(none.is_empty());
    }
}
