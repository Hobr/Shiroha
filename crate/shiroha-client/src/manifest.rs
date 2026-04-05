use anyhow::Context;
use serde_json::Value;

pub(crate) fn parse_json_value_required(raw: &str, field_name: &str) -> anyhow::Result<Value> {
    serde_json::from_str(raw).with_context(|| format!("failed to parse `{field_name}` as JSON"))
}

pub(crate) fn manifest_event_names(manifest: &Value) -> Vec<String> {
    let mut events = manifest
        .get("transitions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|transition| transition.get("event").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    events.sort_unstable();
    events.dedup();
    events
}

pub(crate) fn manifest_state_names(manifest: &Value) -> Vec<String> {
    let mut states = manifest
        .get("states")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|state| state.get("name").and_then(Value::as_str))
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    states.sort_unstable();
    states.dedup();
    states
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn manifest_helpers_extract_deduped_names() {
        let manifest = json!({
            "states": [{"name": "idle"}, {"name": "done"}, {"name": "idle"}],
            "transitions": [{"event": "approve"}, {"event": "reject"}, {"event": "approve"}]
        });

        assert_eq!(manifest_state_names(&manifest), vec!["done", "idle"]);
        assert_eq!(manifest_event_names(&manifest), vec!["approve", "reject"]);
    }

    #[test]
    fn parse_json_value_required_returns_contextful_error() {
        let error =
            parse_json_value_required("{", "manifest").expect_err("invalid JSON should fail");

        assert!(
            error
                .to_string()
                .contains("failed to parse `manifest` as JSON")
        );
    }
}
