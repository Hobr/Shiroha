use serde_json::Value;

pub(crate) fn parse_json_value(raw: &str) -> Option<Value> {
    serde_json::from_str(raw).ok()
}

pub(crate) fn manifest_event_names(raw: &str) -> Vec<String> {
    let Some(value) = parse_json_value(raw) else {
        return Vec::new();
    };

    let mut events = value
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

pub(crate) fn manifest_state_names(raw: &str) -> Vec<String> {
    let Some(value) = parse_json_value(raw) else {
        return Vec::new();
    };

    let mut states = value
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
        let raw = serde_json::to_string(&manifest).expect("manifest json");

        assert_eq!(manifest_state_names(&raw), vec!["done", "idle"]);
        assert_eq!(manifest_event_names(&raw), vec!["approve", "reject"]);
    }
}
