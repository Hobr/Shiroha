use serde_json::{Value, json};
use shiroha_client::JobEvent;

use crate::presenter_support::{compact_json, print_json_block, print_json_value};

pub(crate) fn render_events(
    events: &[JobEvent],
    pretty: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    if json_output {
        return print_json_value(&events_to_json_value(events));
    }

    if !pretty {
        println!("{:<38} {:<16} KIND", "ID", "TIMESTAMP_MS");
        for event in events {
            println!(
                "{:<38} {:<16} {}",
                event.id,
                event.timestamp_ms,
                compact_json(&event.kind)
            );
        }
        return Ok(());
    }

    for event in events {
        println!("id:           {}", event.id);
        println!("timestamp_ms: {}", event.timestamp_ms);
        println!("kind:");
        print_json_block(&event.kind, true);
        println!();
    }
    Ok(())
}

pub(crate) fn render_empty_events(json_output: bool) -> anyhow::Result<()> {
    if json_output {
        return print_json_value(&Value::Array(Vec::new()));
    }

    println!("no events");
    Ok(())
}

fn events_to_json_value(events: &[JobEvent]) -> Value {
    Value::Array(events.iter().map(event_to_json_value).collect())
}

fn event_to_json_value(event: &JobEvent) -> Value {
    json!({
        "id": event.id,
        "job_id": event.job_id,
        "timestamp_ms": event.timestamp_ms,
        "kind": event.kind.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_to_json_value_returns_array() {
        let value = events_to_json_value(&[JobEvent {
            id: "event-1".into(),
            job_id: "job-1".into(),
            timestamp_ms: 42,
            kind: json!({"type": "created"}),
        }]);

        assert_eq!(
            value,
            json!([{
                "id": "event-1",
                "job_id": "job-1",
                "timestamp_ms": 42,
                "kind": {
                    "type": "created"
                }
            }])
        );
    }
}
