use serde_json::{Value, json};
use shiroha_client::{ForceDeleteJobResult, JobDetails};

use crate::presenter_support::{format_optional_u64, print_json_value};

pub(crate) fn render_create_job_result(
    job_id: &str,
    flow_id: &str,
    context: Option<&[u8]>,
    json_output: bool,
) -> anyhow::Result<()> {
    if json_output {
        return print_json_value(&json!({
            "job_id": job_id,
            "flow_id": flow_id,
            "context_bytes": context.map(|bytes| bytes.len()),
        }));
    }

    println!(
        "created job_id={job_id}{}",
        context
            .map(|bytes| format!(" context_bytes={}", bytes.len()))
            .unwrap_or_default()
    );
    Ok(())
}

pub(crate) fn render_job(job: &JobDetails, json_output: bool) -> anyhow::Result<()> {
    if json_output {
        return print_json_value(&job_to_json_value(job));
    }

    println!("job_id:        {}", job.job_id);
    println!("flow_id:       {}", job.flow_id);
    println!("flow_version:  {}", job.flow_version);
    println!("state:         {}", job.state);
    println!("current_state: {}", job.current_state);
    println!("context_bytes: {}", format_optional_u64(job.context_bytes));
    Ok(())
}

pub(crate) fn render_jobs(jobs: &[JobDetails], json_output: bool) -> anyhow::Result<()> {
    if jobs.is_empty() {
        if json_output {
            return print_json_value(&Value::Array(Vec::new()));
        }
        println!("no jobs");
        return Ok(());
    }

    if json_output {
        return print_json_value(&jobs_to_json_value(jobs));
    }

    println!(
        "{:<38} {:<20} {:<38} {:<12} {:<13} CURRENT",
        "JOB_ID", "FLOW_ID", "FLOW_VERSION", "STATE", "CONTEXT_BYTES"
    );
    for job in jobs {
        println!(
            "{:<38} {:<20} {:<38} {:<12} {:<13} {}",
            job.job_id,
            job.flow_id,
            job.flow_version,
            job.state,
            format_optional_u64(job.context_bytes),
            job.current_state
        );
    }
    Ok(())
}

pub(crate) fn render_delete_job(job_id: &str, json_output: bool) -> anyhow::Result<()> {
    if json_output {
        return print_json_value(&json!({
            "job_id": job_id,
            "operation": "delete_job",
            "forced": false,
        }));
    }

    println!("job {job_id} deleted");
    Ok(())
}

pub(crate) fn render_force_delete_job(
    result: &ForceDeleteJobResult,
    json_output: bool,
) -> anyhow::Result<()> {
    if json_output {
        return print_json_value(&json!({
            "job_id": result.job_id,
            "operation": "delete_job",
            "forced": true,
            "previous_state": result.previous_state,
            "cancelled_before_delete": result.cancelled_before_delete,
        }));
    }

    println!(
        "job {} force-deleted (previous_state={} auto_cancelled={})",
        result.job_id, result.previous_state, result.cancelled_before_delete
    );
    Ok(())
}

pub(crate) fn render_trigger_event(
    job_id: &str,
    event: &str,
    payload: Option<&[u8]>,
    json_output: bool,
) -> anyhow::Result<()> {
    if json_output {
        return print_json_value(&json!({
            "job_id": job_id,
            "event": event,
            "payload_bytes": payload.map(|bytes| bytes.len()),
        }));
    }

    println!(
        "event `{event}` triggered on job {job_id}{}",
        payload
            .map(|bytes| format!(" payload_bytes={}", bytes.len()))
            .unwrap_or_default()
    );
    Ok(())
}

pub(crate) fn render_job_operation(
    job_id: &str,
    operation: &str,
    json_output: bool,
) -> anyhow::Result<()> {
    if json_output {
        return print_json_value(&json!({
            "job_id": job_id,
            "operation": operation,
        }));
    }

    let text = match operation {
        "pause" => "paused",
        "resume" => "resumed",
        "cancel" => "cancelled",
        _ => operation,
    };
    println!("job {job_id} {text}");
    Ok(())
}

fn jobs_to_json_value(jobs: &[JobDetails]) -> Value {
    Value::Array(jobs.iter().map(job_to_json_value).collect())
}

fn job_to_json_value(job: &JobDetails) -> Value {
    json!({
        "job_id": job.job_id,
        "flow_id": job.flow_id,
        "state": job.state,
        "current_state": job.current_state,
        "flow_version": job.flow_version,
        "context_bytes": job.context_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_to_json_value_returns_array() {
        let value = jobs_to_json_value(&[JobDetails {
            job_id: "job-1".into(),
            flow_id: "flow-a".into(),
            state: "running".into(),
            current_state: "idle".into(),
            flow_version: "version-a".into(),
            context_bytes: Some(42),
        }]);

        assert_eq!(
            value,
            json!([{
                "job_id": "job-1",
                "flow_id": "flow-a",
                "state": "running",
                "current_state": "idle",
                "flow_version": "version-a",
                "context_bytes": 42
            }])
        );
    }
}
