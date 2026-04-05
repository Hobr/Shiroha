use serde_json::{Value, json};
use shiroha_client::{
    FlowDetails, FlowVersionSummary, ForceDeleteFlowResult, ForceDeleteJobResult,
};

use crate::presenter_support::{print_json_block, print_json_value, value_to_label};

pub(crate) fn render_deploy_result(
    flow_id: &str,
    version: &str,
    flow: Option<&FlowDetails>,
    warnings: &[String],
    json_output: bool,
) -> anyhow::Result<()> {
    if json_output {
        return print_json_value(&json!({
            "flow_id": flow_id,
            "version": version,
            "manifest": flow.map(|flow| flow.manifest.clone()),
            "warnings": warnings,
        }));
    }

    println!("deployed flow_id={flow_id} version={version}");
    if let Some(flow) = flow
        && let Some(summary) = manifest_summary(&flow.manifest)
    {
        println!(
            "initial_state={} states={} transitions={} actions={}",
            summary.initial_state,
            summary.state_count,
            summary.transition_count,
            summary.action_count
        );
    }
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
    Ok(())
}

pub(crate) fn render_flow_list(
    flows: &[FlowVersionSummary],
    empty_message: &str,
    json_output: bool,
) -> anyhow::Result<()> {
    if flows.is_empty() {
        if json_output {
            return print_json_value(&Value::Array(Vec::new()));
        }
        println!("{empty_message}");
        return Ok(());
    }

    if json_output {
        return print_json_value(&flow_summaries_to_json_value(flows));
    }

    println!(
        "{:<20} {:<38} {:<15} STATES",
        "FLOW_ID", "VERSION", "INITIAL"
    );
    for flow in flows {
        println!(
            "{:<20} {:<38} {:<15} {}",
            flow.flow_id, flow.version, flow.initial_state, flow.state_count
        );
    }
    Ok(())
}

pub(crate) fn render_flow_details(
    flow: &FlowDetails,
    summary: bool,
    json_output: bool,
) -> anyhow::Result<()> {
    if json_output {
        return print_json_value(&json!({
            "flow_id": flow.flow_id,
            "version": flow.version,
            "manifest": flow.manifest.clone(),
        }));
    }

    if summary {
        print_flow_summary(flow);
        return Ok(());
    }

    println!("flow_id:  {}", flow.flow_id);
    println!("version:  {}", flow.version);
    println!("manifest:");
    print_json_block(&flow.manifest, true);
    Ok(())
}

pub(crate) fn render_delete_flow(flow_id: &str, json_output: bool) -> anyhow::Result<()> {
    if json_output {
        return print_json_value(&json!({
            "flow_id": flow_id,
            "operation": "delete_flow",
            "forced": false,
        }));
    }

    println!("flow {flow_id} deleted");
    Ok(())
}

pub(crate) fn render_force_delete_flow(
    result: &ForceDeleteFlowResult,
    json_output: bool,
) -> anyhow::Result<()> {
    if json_output {
        return print_json_value(&json!({
            "flow_id": result.flow_id,
            "operation": "delete_flow",
            "forced": true,
            "deleted_jobs": result
                .deleted_jobs
                .iter()
                .map(force_delete_job_to_json)
                .collect::<Vec<_>>(),
        }));
    }

    if result.deleted_jobs.is_empty() {
        println!("flow {} force-deleted (no jobs)", result.flow_id);
        return Ok(());
    }

    println!(
        "flow {} force-deleted after deleting {} job(s):",
        result.flow_id,
        result.deleted_jobs.len()
    );
    for job in &result.deleted_jobs {
        println!(
            "  {} previous_state={} auto_cancelled={}",
            job.job_id, job.previous_state, job.cancelled_before_delete
        );
    }
    Ok(())
}

fn flow_summaries_to_json_value(flows: &[FlowVersionSummary]) -> Value {
    Value::Array(
        flows
            .iter()
            .map(|flow| {
                json!({
                    "flow_id": flow.flow_id,
                    "version": flow.version,
                    "initial_state": flow.initial_state,
                    "state_count": flow.state_count,
                })
            })
            .collect(),
    )
}

fn force_delete_job_to_json(result: &ForceDeleteJobResult) -> Value {
    json!({
        "job_id": result.job_id,
        "previous_state": result.previous_state,
        "cancelled_before_delete": result.cancelled_before_delete,
    })
}

struct ManifestSummary {
    initial_state: String,
    state_count: usize,
    transition_count: usize,
    action_count: usize,
}

fn manifest_summary(manifest: &Value) -> Option<ManifestSummary> {
    Some(ManifestSummary {
        initial_state: manifest.get("initial_state")?.as_str()?.to_string(),
        state_count: manifest.get("states")?.as_array()?.len(),
        transition_count: manifest.get("transitions")?.as_array()?.len(),
        action_count: manifest.get("actions")?.as_array()?.len(),
    })
}

struct FlowTopologySummary {
    initial_state: String,
    states: Vec<FlowStateSummary>,
    transitions: Vec<FlowTransitionSummary>,
    actions: Vec<FlowActionSummary>,
}

struct FlowStateSummary {
    name: String,
    kind: String,
    on_enter: Option<String>,
    on_exit: Option<String>,
    subprocess_flow_id: Option<String>,
    completion_event: Option<String>,
}

struct FlowTransitionSummary {
    from: String,
    event: String,
    to: String,
    guard: Option<String>,
    action: Option<String>,
    timeout: Option<String>,
}

struct FlowActionSummary {
    name: String,
    dispatch: String,
}

fn print_flow_summary(flow: &FlowDetails) {
    let Some(summary) = flow_topology_summary(&flow.manifest) else {
        println!("flow_id:  {}", flow.flow_id);
        println!("version:  {}", flow.version);
        println!("manifest:");
        print_json_block(&flow.manifest, true);
        return;
    };

    println!("flow_id:       {}", flow.flow_id);
    println!("version:       {}", flow.version);
    println!("initial_state: {}", summary.initial_state);
    println!("states:        {}", summary.states.len());
    println!("transitions:   {}", summary.transitions.len());
    println!("actions:       {}", summary.actions.len());
    println!();
    println!("states:");
    for state in &summary.states {
        let mut extras = Vec::new();
        if let Some(on_enter) = state.on_enter.as_deref() {
            extras.push(format!("on_enter={on_enter}"));
        }
        if let Some(on_exit) = state.on_exit.as_deref() {
            extras.push(format!("on_exit={on_exit}"));
        }
        if let Some(subprocess_flow_id) = state.subprocess_flow_id.as_deref() {
            let completion_event = state.completion_event.as_deref().unwrap_or("<missing>");
            extras.push(format!(
                "subprocess.flow_id={subprocess_flow_id} completion_event={completion_event}"
            ));
        }
        if extras.is_empty() {
            println!("  - {} [{}]", state.name, state.kind);
        } else {
            println!("  - {} [{}] {}", state.name, state.kind, extras.join(" "));
        }
    }
    println!();
    println!("transitions:");
    for transition in &summary.transitions {
        let mut extras = Vec::new();
        if let Some(guard) = transition.guard.as_deref() {
            extras.push(format!("guard={guard}"));
        }
        if let Some(action) = transition.action.as_deref() {
            extras.push(format!("action={action}"));
        }
        if let Some(timeout) = transition.timeout.as_deref() {
            extras.push(format!("timeout={timeout}"));
        }
        if extras.is_empty() {
            println!(
                "  - {} --{}--> {}",
                transition.from, transition.event, transition.to
            );
        } else {
            println!(
                "  - {} --{}--> {} {}",
                transition.from,
                transition.event,
                transition.to,
                extras.join(" ")
            );
        }
    }
    println!();
    println!("actions:");
    for action in &summary.actions {
        println!("  - {} dispatch={}", action.name, action.dispatch);
    }
}

fn flow_topology_summary(manifest: &Value) -> Option<FlowTopologySummary> {
    Some(FlowTopologySummary {
        initial_state: manifest.get("initial_state")?.as_str()?.to_string(),
        states: manifest
            .get("states")?
            .as_array()?
            .iter()
            .map(flow_state_summary)
            .collect::<Option<Vec<_>>>()?,
        transitions: manifest
            .get("transitions")?
            .as_array()?
            .iter()
            .map(flow_transition_summary)
            .collect::<Option<Vec<_>>>()?,
        actions: manifest
            .get("actions")?
            .as_array()?
            .iter()
            .map(flow_action_summary)
            .collect::<Option<Vec<_>>>()?,
    })
}

fn flow_state_summary(value: &Value) -> Option<FlowStateSummary> {
    let subprocess = value.get("subprocess");
    Some(FlowStateSummary {
        name: value.get("name")?.as_str()?.to_string(),
        kind: value.get("kind")?.as_str()?.to_string(),
        on_enter: value
            .get("on_enter")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        on_exit: value
            .get("on_exit")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        subprocess_flow_id: subprocess
            .and_then(|value| value.get("flow_id"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
        completion_event: subprocess
            .and_then(|value| value.get("completion_event"))
            .and_then(Value::as_str)
            .map(ToString::to_string),
    })
}

fn flow_transition_summary(value: &Value) -> Option<FlowTransitionSummary> {
    Some(FlowTransitionSummary {
        from: value.get("from")?.as_str()?.to_string(),
        event: value.get("event")?.as_str()?.to_string(),
        to: value.get("to")?.as_str()?.to_string(),
        guard: value
            .get("guard")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        action: value
            .get("action")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        timeout: value.get("timeout").and_then(format_timeout),
    })
}

fn flow_action_summary(value: &Value) -> Option<FlowActionSummary> {
    Some(FlowActionSummary {
        name: value.get("name")?.as_str()?.to_string(),
        dispatch: value
            .get("dispatch")
            .map(value_to_label)
            .unwrap_or_else(|| "unknown".to_string()),
    })
}

fn format_timeout(value: &Value) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let duration_ms = value.get("duration_ms")?.as_u64()?;
    let timeout_event = value.get("timeout_event")?.as_str()?;
    Some(format!("{duration_ms}ms=>{timeout_event}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn manifest_summary_extracts_counts() {
        let summary = manifest_summary(&json!({
            "initial_state": "idle",
            "states": [{}, {}],
            "transitions": [{}],
            "actions": [{}, {}],
        }))
        .expect("summary should parse");

        assert_eq!(summary.initial_state, "idle");
        assert_eq!(summary.state_count, 2);
        assert_eq!(summary.transition_count, 1);
        assert_eq!(summary.action_count, 2);
    }

    #[test]
    fn flow_topology_summary_extracts_hooks_and_subprocess() {
        let summary = flow_topology_summary(&json!({
            "initial_state": "review",
            "states": [{
                "name": "review",
                "kind": "subprocess",
                "on_enter": "enter-review",
                "subprocess": {
                    "flow_id": "child-flow",
                    "completion_event": "child-done"
                }
            }],
            "transitions": [{
                "from": "review",
                "event": "child-done",
                "to": "approved",
                "action": "finalize",
                "timeout": {
                    "duration_ms": 5000,
                    "timeout_event": "expire"
                }
            }],
            "actions": [{
                "name": "finalize",
                "dispatch": "local"
            }],
        }))
        .expect("summary should parse");

        assert_eq!(summary.initial_state, "review");
        assert_eq!(
            summary.states[0].subprocess_flow_id.as_deref(),
            Some("child-flow")
        );
        assert_eq!(
            summary.states[0].completion_event.as_deref(),
            Some("child-done")
        );
        assert_eq!(
            summary.transitions[0].timeout.as_deref(),
            Some("5000ms=>expire")
        );
        assert_eq!(summary.actions[0].dispatch, "local");
    }
}
