//! gRPC 客户端封装
//!
//! 封装 FlowServiceClient 和 JobServiceClient，为每个 CLI 子命令提供对应方法。

use std::path::Path;

use anyhow::{Context, bail};
use sctl::control::{ControlClient, EventQuery};
use serde_json::{Value, json};
use shiroha_proto::shiroha_api::*;
use tokio::time::{Duration, sleep};

/// shirohad gRPC 客户端
pub struct ShirohaClient {
    api: ControlClient,
}

pub struct EventQueryOptions {
    pub pretty: bool,
    pub follow: bool,
    pub since_id: Option<String>,
    pub since_timestamp_ms: Option<u64>,
    pub limit: Option<u32>,
    pub kind_filters: Vec<String>,
    pub tail: Option<usize>,
    pub interval_ms: u64,
    pub json_output: bool,
}

impl ShirohaClient {
    /// 连接到 shirohad gRPC 服务
    pub async fn connect(addr: &str) -> anyhow::Result<Self> {
        Ok(Self {
            api: ControlClient::connect(addr).await?,
        })
    }

    /// 部署 Flow：读取本地 WASM 文件并上传
    pub async fn deploy(
        &mut self,
        flow_id: &str,
        file: &str,
        json_output: bool,
    ) -> anyhow::Result<()> {
        let wasm_bytes = std::fs::read(file)?;
        let resp = self.api.deploy_flow(flow_id, wasm_bytes).await?;

        let flow_details = self.api.get_flow(&resp.flow_id, None).await.ok();

        if json_output {
            let manifest = flow_details
                .as_ref()
                .and_then(|flow| parse_json_value(&flow.manifest_json));
            print_json_value(&json!({
                "flow_id": resp.flow_id,
                "version": resp.version,
                "manifest": manifest,
                "warnings": resp.warnings,
            }))?;
            return Ok(());
        }

        println!("deployed flow_id={} version={}", resp.flow_id, resp.version);
        if let Some(flow) = flow_details
            && let Some(summary) = manifest_summary(&flow.manifest_json)
        {
            println!(
                "initial_state={} states={} transitions={} actions={}",
                summary.initial_state,
                summary.state_count,
                summary.transition_count,
                summary.action_count
            );
        }
        for warning in &resp.warnings {
            eprintln!("warning: {warning}");
        }
        Ok(())
    }

    pub async fn list_flows(&mut self, json_output: bool) -> anyhow::Result<()> {
        let flows = self.api.list_flows().await?;
        if flows.is_empty() {
            if json_output {
                return print_json_value(&Value::Array(Vec::new()));
            }
            println!("no flows");
            return Ok(());
        }
        if json_output {
            print_json_value(&json!(
                flows
                    .iter()
                    .map(|flow| json!({
                        "flow_id": flow.flow_id,
                        "version": flow.version,
                        "initial_state": flow.initial_state,
                        "state_count": flow.state_count,
                    }))
                    .collect::<Vec<_>>()
            ))?;
            return Ok(());
        }
        // 保持纯文本表格输出，避免给 shell 管道增加额外格式依赖。
        println!(
            "{:<20} {:<38} {:<15} STATES",
            "FLOW_ID", "VERSION", "INITIAL"
        );
        for f in &flows {
            println!(
                "{:<20} {:<38} {:<15} {}",
                f.flow_id, f.version, f.initial_state, f.state_count
            );
        }
        Ok(())
    }
    pub async fn get_flow(
        &mut self,
        flow_id: &str,
        version: Option<&str>,
        summary: bool,
        json_output: bool,
    ) -> anyhow::Result<()> {
        let resp = self.api.get_flow(flow_id, version).await?;
        if json_output {
            print_json_value(&json!({
                "flow_id": resp.flow_id,
                "version": resp.version,
                "manifest": parse_json_value(&resp.manifest_json).unwrap_or(Value::String(resp.manifest_json)),
            }))?;
            return Ok(());
        }
        if summary {
            print_flow_summary(&resp.flow_id, &resp.version, &resp.manifest_json);
            return Ok(());
        }
        println!("flow_id:  {}", resp.flow_id);
        println!("version:  {}", resp.version);
        println!("manifest:");
        print_json_block(&resp.manifest_json, true);
        Ok(())
    }

    pub async fn list_flow_versions(
        &mut self,
        flow_id: &str,
        json_output: bool,
    ) -> anyhow::Result<()> {
        let flows = self.api.list_flow_versions(flow_id).await?;
        if flows.is_empty() {
            if json_output {
                return print_json_value(&Value::Array(Vec::new()));
            }
            println!("no historical versions");
            return Ok(());
        }
        if json_output {
            print_json_value(&json!(
                flows
                    .iter()
                    .map(|flow| json!({
                        "flow_id": flow.flow_id,
                        "version": flow.version,
                        "initial_state": flow.initial_state,
                        "state_count": flow.state_count,
                    }))
                    .collect::<Vec<_>>()
            ))?;
            return Ok(());
        }
        println!(
            "{:<20} {:<38} {:<15} STATES",
            "FLOW_ID", "VERSION", "INITIAL"
        );
        for flow in &flows {
            println!(
                "{:<20} {:<38} {:<15} {}",
                flow.flow_id, flow.version, flow.initial_state, flow.state_count
            );
        }
        Ok(())
    }

    pub async fn delete_flow(&mut self, flow_id: &str, json_output: bool) -> anyhow::Result<()> {
        let resp = self.api.delete_flow(flow_id).await?;
        if json_output {
            print_json_value(&json!({
                "flow_id": resp.flow_id,
                "operation": "delete_flow",
            }))?;
            return Ok(());
        }
        println!("flow {} deleted", resp.flow_id);
        Ok(())
    }

    pub async fn create_job(
        &mut self,
        flow_id: &str,
        context: Option<Vec<u8>>,
        json_output: bool,
    ) -> anyhow::Result<()> {
        let resp = self.api.create_job(flow_id, context.clone()).await?;
        if json_output {
            print_json_value(&json!({
                "job_id": resp.job_id,
                "flow_id": flow_id,
                "context_bytes": context.as_ref().map(|bytes| bytes.len()),
            }))?;
            return Ok(());
        }
        println!(
            "created job_id={}{}",
            resp.job_id,
            context
                .as_ref()
                .map(|bytes| format!(" context_bytes={}", bytes.len()))
                .unwrap_or_default()
        );
        Ok(())
    }

    pub async fn get_job(&mut self, job_id: &str, json_output: bool) -> anyhow::Result<()> {
        let resp = self.api.get_job(job_id).await?;
        if json_output {
            print_job_json(&resp)?;
            return Ok(());
        }
        print_job_text(&resp);
        Ok(())
    }

    pub async fn delete_job(&mut self, job_id: &str, json_output: bool) -> anyhow::Result<()> {
        let resp = self.api.delete_job(job_id).await?;
        if json_output {
            print_json_value(&json!({
                "job_id": resp.job_id,
                "operation": "delete_job",
            }))?;
            return Ok(());
        }
        println!("job {} deleted", resp.job_id);
        Ok(())
    }

    pub async fn list_jobs(
        &mut self,
        flow_id: Option<&str>,
        all: bool,
        json_output: bool,
    ) -> anyhow::Result<()> {
        let mut jobs = if all {
            self.api.list_all_jobs().await?
        } else {
            self.api
                .list_jobs_for_flow(
                    flow_id.expect("clap should require --flow-id when --all is absent"),
                )
                .await?
        };
        if jobs.is_empty() {
            if json_output {
                return print_json_value(&Value::Array(Vec::new()));
            }
            println!("no jobs");
            return Ok(());
        }
        sort_jobs(&mut jobs);
        if json_output {
            print_json_value(&jobs_to_json_value(&jobs))?;
            return Ok(());
        }
        println!(
            "{:<38} {:<20} {:<38} {:<12} {:<13} CURRENT",
            "JOB_ID", "FLOW_ID", "FLOW_VERSION", "STATE", "CONTEXT_BYTES"
        );
        for j in &jobs {
            println!(
                "{:<38} {:<20} {:<38} {:<12} {:<13} {}",
                j.job_id,
                j.flow_id,
                j.flow_version,
                j.state,
                j.context_bytes
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                j.current_state
            );
        }
        Ok(())
    }
    pub async fn trigger_event(
        &mut self,
        job_id: &str,
        event: &str,
        payload: Option<Vec<u8>>,
        json_output: bool,
    ) -> anyhow::Result<()> {
        self.api
            .trigger_event(job_id, event, payload.clone())
            .await?;
        if json_output {
            print_json_value(&json!({
                "job_id": job_id,
                "event": event,
                "payload_bytes": payload.as_ref().map(|bytes| bytes.len()),
            }))?;
            return Ok(());
        }
        println!(
            "event `{event}` triggered on job {job_id}{}",
            payload
                .as_ref()
                .map(|bytes| format!(" payload_bytes={}", bytes.len()))
                .unwrap_or_default()
        );
        Ok(())
    }

    pub async fn pause_job(&mut self, job_id: &str, json_output: bool) -> anyhow::Result<()> {
        self.api.pause_job(job_id).await?;
        if json_output {
            print_json_value(&json!({
                "job_id": job_id,
                "operation": "pause",
            }))?;
            return Ok(());
        }
        println!("job {job_id} paused");
        Ok(())
    }

    pub async fn resume_job(&mut self, job_id: &str, json_output: bool) -> anyhow::Result<()> {
        self.api.resume_job(job_id).await?;
        if json_output {
            print_json_value(&json!({
                "job_id": job_id,
                "operation": "resume",
            }))?;
            return Ok(());
        }
        println!("job {job_id} resumed");
        Ok(())
    }

    pub async fn cancel_job(&mut self, job_id: &str, json_output: bool) -> anyhow::Result<()> {
        self.api.cancel_job(job_id).await?;
        if json_output {
            print_json_value(&json!({
                "job_id": job_id,
                "operation": "cancel",
            }))?;
            return Ok(());
        }
        println!("job {job_id} cancelled");
        Ok(())
    }

    pub async fn get_job_events(
        &mut self,
        job_id: &str,
        options: EventQueryOptions,
    ) -> anyhow::Result<()> {
        if options.follow {
            return self.follow_job_events(job_id, &options).await;
        }

        let events = apply_tail(
            self.api
                .get_job_events(
                    job_id,
                    &EventQuery {
                        since_id: options.since_id.clone(),
                        since_timestamp_ms: options.since_timestamp_ms,
                        limit: options.limit,
                        kind_filters: options.kind_filters.clone(),
                    },
                )
                .await?,
            options.tail,
        );
        if events.is_empty() {
            if options.json_output {
                return print_json_value(&Value::Array(Vec::new()));
            }
            println!("no events");
            return Ok(());
        }
        render_events(&events, options.pretty, options.json_output)?;
        Ok(())
    }

    pub async fn wait_job(
        &mut self,
        job_id: &str,
        target_state: Option<&str>,
        timeout_ms: Option<u64>,
        interval_ms: u64,
        json_output: bool,
    ) -> anyhow::Result<()> {
        let wait_future = async {
            loop {
                let job = self.api.get_job(job_id).await?;
                if job_matches_target(&job, target_state) {
                    return Ok::<GetJobResponse, anyhow::Error>(job);
                }
                sleep(Duration::from_millis(interval_ms)).await;
            }
        };

        let job = if let Some(timeout_ms) = timeout_ms {
            tokio::time::timeout(Duration::from_millis(timeout_ms), wait_future)
                .await
                .with_context(|| {
                    format!(
                        "timed out waiting for job `{job_id}` to reach {}",
                        target_state.unwrap_or("a terminal state")
                    )
                })??
        } else {
            wait_future.await?
        };

        if json_output {
            print_job_json(&job)?;
        } else {
            print_job_text(&job);
        }
        Ok(())
    }

    async fn follow_job_events(
        &mut self,
        job_id: &str,
        options: &EventQueryOptions,
    ) -> anyhow::Result<()> {
        let mut since_id = options.since_id.clone();
        let mut since_timestamp_ms = options.since_timestamp_ms;
        loop {
            let new_events = apply_tail(
                self.api
                    .get_job_events(
                        job_id,
                        &EventQuery {
                            since_id: since_id.clone(),
                            since_timestamp_ms,
                            limit: options.limit,
                            kind_filters: options.kind_filters.clone(),
                        },
                    )
                    .await?,
                options.tail,
            );

            if !new_events.is_empty() {
                since_id = new_events.last().map(|event| event.id.clone());
                since_timestamp_ms = None;
                if options.json_output {
                    print_json_value(&events_to_json_value(&new_events))?;
                } else {
                    render_events(&new_events, options.pretty, false)?;
                }
            }

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    return Ok(());
                }
                _ = sleep(Duration::from_millis(options.interval_ms)) => {}
            }
        }
    }
}

pub fn decode_optional_bytes(
    text: Option<&str>,
    hex: Option<&str>,
    file: Option<&str>,
) -> anyhow::Result<Option<Vec<u8>>> {
    match (text, hex, file) {
        (Some(text), None, None) => Ok(Some(text.as_bytes().to_vec())),
        (None, Some(hex), None) => Ok(Some(decode_hex(hex)?)),
        (None, None, Some(path)) => {
            Ok(Some(std::fs::read(Path::new(path)).with_context(|| {
                format!("failed to read bytes from `{path}`")
            })?))
        }
        (None, None, None) => Ok(None),
        _ => bail!("multiple byte input sources were provided"),
    }
}

fn decode_hex(input: &str) -> anyhow::Result<Vec<u8>> {
    let filtered: String = input
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace() && *ch != '_')
        .collect();
    if !filtered.len().is_multiple_of(2) {
        bail!("hex input must contain an even number of digits");
    }

    let mut bytes = Vec::with_capacity(filtered.len() / 2);
    let chars: Vec<char> = filtered.chars().collect();
    for chunk in chars.chunks(2) {
        let pair: String = chunk.iter().collect();
        let byte =
            u8::from_str_radix(&pair, 16).with_context(|| format!("invalid hex byte `{pair}`"))?;
        bytes.push(byte);
    }
    Ok(bytes)
}

fn compact_json(raw: &str) -> String {
    serde_json::from_str::<Value>(raw)
        .and_then(|value| serde_json::to_string(&value))
        .unwrap_or_else(|_| raw.to_string())
}

fn parse_json_value(raw: &str) -> Option<Value> {
    serde_json::from_str(raw).ok()
}

fn print_json_value(value: &Value) -> anyhow::Result<()> {
    println!(
        "{}",
        serde_json::to_string(value).context("failed to serialize JSON output")?
    );
    Ok(())
}

fn print_json_block(raw: &str, pretty: bool) {
    match serde_json::from_str::<Value>(raw) {
        Ok(value) if pretty => match serde_json::to_string_pretty(&value) {
            Ok(formatted) => {
                for line in formatted.lines() {
                    println!("  {line}");
                }
            }
            Err(_) => println!("  {raw}"),
        },
        Ok(value) => match serde_json::to_string(&value) {
            Ok(formatted) => println!("  {formatted}"),
            Err(_) => println!("  {raw}"),
        },
        Err(_) => println!("  {raw}"),
    }
}

fn render_events(events: &[EventRecord], pretty: bool, json_output: bool) -> anyhow::Result<()> {
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
                compact_json(&event.kind_json)
            );
        }
        return Ok(());
    }

    for event in events {
        println!("id:           {}", event.id);
        println!("timestamp_ms: {}", event.timestamp_ms);
        println!("kind:");
        print_json_block(&event.kind_json, true);
        println!();
    }
    Ok(())
}

fn event_to_json_value(event: &EventRecord) -> Value {
    json!({
        "id": event.id,
        "job_id": event.job_id,
        "timestamp_ms": event.timestamp_ms,
        "kind": parse_json_value(&event.kind_json).unwrap_or(Value::String(event.kind_json.clone())),
    })
}

fn events_to_json_value(events: &[EventRecord]) -> Value {
    Value::Array(events.iter().map(event_to_json_value).collect())
}

fn jobs_to_json_value(jobs: &[GetJobResponse]) -> Value {
    Value::Array(
        jobs.iter()
            .map(|job| {
                json!({
                    "job_id": job.job_id,
                    "flow_id": job.flow_id,
                    "state": job.state,
                    "current_state": job.current_state,
                    "flow_version": job.flow_version,
                    "context_bytes": job.context_bytes,
                })
            })
            .collect(),
    )
}

fn print_job_json(job: &GetJobResponse) -> anyhow::Result<()> {
    print_json_value(&json!({
        "job_id": job.job_id,
        "flow_id": job.flow_id,
        "state": job.state,
        "current_state": job.current_state,
        "flow_version": job.flow_version,
        "context_bytes": job.context_bytes,
    }))
}

fn print_job_text(job: &GetJobResponse) {
    println!("job_id:        {}", job.job_id);
    println!("flow_id:       {}", job.flow_id);
    println!("flow_version:  {}", job.flow_version);
    println!("state:         {}", job.state);
    println!("current_state: {}", job.current_state);
    println!(
        "context_bytes: {}",
        job.context_bytes
            .map(|value| value.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
}

fn job_matches_target(job: &GetJobResponse, target_state: Option<&str>) -> bool {
    match target_state {
        Some(target) => job.state == target || job.current_state == target,
        None => matches!(job.state.as_str(), "completed" | "cancelled"),
    }
}

fn sort_jobs(jobs: &mut [GetJobResponse]) {
    jobs.sort_by(|left, right| {
        left.flow_id
            .cmp(&right.flow_id)
            .then_with(|| left.job_id.cmp(&right.job_id))
    });
}

fn apply_tail(mut events: Vec<EventRecord>, tail: Option<usize>) -> Vec<EventRecord> {
    if let Some(tail) = tail
        && events.len() > tail
    {
        events = events.split_off(events.len() - tail);
    }
    events
}

struct ManifestSummary {
    initial_state: String,
    state_count: usize,
    transition_count: usize,
    action_count: usize,
}

fn manifest_summary(raw: &str) -> Option<ManifestSummary> {
    let value = parse_json_value(raw)?;
    Some(ManifestSummary {
        initial_state: value.get("initial_state")?.as_str()?.to_string(),
        state_count: value.get("states")?.as_array()?.len(),
        transition_count: value.get("transitions")?.as_array()?.len(),
        action_count: value.get("actions")?.as_array()?.len(),
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

fn print_flow_summary(flow_id: &str, version: &str, raw: &str) {
    let Some(summary) = flow_topology_summary(raw) else {
        println!("flow_id:  {flow_id}");
        println!("version:  {version}");
        println!("manifest:");
        print_json_block(raw, true);
        return;
    };

    println!("flow_id:       {flow_id}");
    println!("version:       {version}");
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

fn flow_topology_summary(raw: &str) -> Option<FlowTopologySummary> {
    let value = parse_json_value(raw)?;
    Some(FlowTopologySummary {
        initial_state: value.get("initial_state")?.as_str()?.to_string(),
        states: value
            .get("states")?
            .as_array()?
            .iter()
            .map(flow_state_summary)
            .collect::<Option<Vec<_>>>()?,
        transitions: value
            .get("transitions")?
            .as_array()?
            .iter()
            .map(flow_transition_summary)
            .collect::<Option<Vec<_>>>()?,
        actions: value
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

fn value_to_label(value: &Value) -> String {
    value
        .as_str()
        .map(ToString::to_string)
        .unwrap_or_else(|| compact_json(&value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_optional_bytes_accepts_text() {
        let decoded =
            decode_optional_bytes(Some("hello"), None, None).expect("text bytes should decode");

        assert_eq!(decoded, Some(b"hello".to_vec()));
    }

    #[test]
    fn decode_optional_bytes_accepts_hex_with_whitespace() {
        let decoded =
            decode_optional_bytes(None, Some("68 65_6c6c6f"), None).expect("hex should decode");

        assert_eq!(decoded, Some(b"hello".to_vec()));
    }

    #[test]
    fn compact_json_normalizes_valid_json() {
        assert_eq!(compact_json("{\"b\":2,\"a\":1}"), "{\"a\":1,\"b\":2}");
    }

    #[test]
    fn job_matches_target_defaults_to_terminal_states() {
        let completed = GetJobResponse {
            job_id: "job-1".into(),
            flow_id: "flow".into(),
            state: "completed".into(),
            current_state: "approved".into(),
            flow_version: "version-1".into(),
            context_bytes: None,
        };
        let cancelled = GetJobResponse {
            job_id: "job-2".into(),
            flow_id: "flow".into(),
            state: "cancelled".into(),
            current_state: "idle".into(),
            flow_version: "version-1".into(),
            context_bytes: Some(0),
        };
        let running = GetJobResponse {
            job_id: "job-3".into(),
            flow_id: "flow".into(),
            state: "running".into(),
            current_state: "waiting-approval".into(),
            flow_version: "version-2".into(),
            context_bytes: Some(12),
        };

        assert!(job_matches_target(&completed, None));
        assert!(job_matches_target(&cancelled, None));
        assert!(!job_matches_target(&running, None));
        assert!(job_matches_target(&running, Some("running")));
        assert!(job_matches_target(&running, Some("waiting-approval")));
    }

    #[test]
    fn manifest_summary_extracts_counts() {
        let summary = manifest_summary(
            r#"{"initial_state":"idle","states":[{},{}],"transitions":[{}],"actions":[{},{}]}"#,
        )
        .expect("summary should parse");

        assert_eq!(summary.initial_state, "idle");
        assert_eq!(summary.state_count, 2);
        assert_eq!(summary.transition_count, 1);
        assert_eq!(summary.action_count, 2);
    }

    #[test]
    fn apply_tail_trims_latest_events() {
        let selected = apply_tail(
            vec![
                EventRecord {
                    id: "event-1".into(),
                    job_id: "job-1".into(),
                    timestamp_ms: 1,
                    kind_json: r#"{"type":"created"}"#.into(),
                },
                EventRecord {
                    id: "event-2".into(),
                    job_id: "job-1".into(),
                    timestamp_ms: 2,
                    kind_json: r#"{"type":"transition"}"#.into(),
                },
                EventRecord {
                    id: "event-3".into(),
                    job_id: "job-1".into(),
                    timestamp_ms: 3,
                    kind_json: r#"{"type":"transition"}"#.into(),
                },
            ],
            Some(1),
        );

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "event-3");
    }

    #[test]
    fn flow_topology_summary_extracts_hooks_and_subprocess() {
        let summary = flow_topology_summary(
            r#"{
                "initial_state":"review",
                "states":[
                    {
                        "name":"review",
                        "kind":"subprocess",
                        "on_enter":"enter-review",
                        "subprocess":{
                            "flow_id":"child-flow",
                            "completion_event":"child-done"
                        }
                    }
                ],
                "transitions":[
                    {
                        "from":"review",
                        "event":"child-done",
                        "to":"approved",
                        "action":"finalize",
                        "timeout":{"duration_ms":5000,"timeout_event":"expire"}
                    }
                ],
                "actions":[
                    {"name":"finalize","dispatch":"local"}
                ]
            }"#,
        )
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

    #[test]
    fn jobs_to_json_value_returns_array() {
        let value = jobs_to_json_value(&[GetJobResponse {
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

    #[test]
    fn events_to_json_value_returns_array() {
        let value = events_to_json_value(&[EventRecord {
            id: "event-1".into(),
            job_id: "job-1".into(),
            timestamp_ms: 42,
            kind_json: r#"{"type":"created"}"#.into(),
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
