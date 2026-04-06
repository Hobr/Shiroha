//! gRPC 客户端封装
//!
//! 封装 `ControlClient`，为每个 CLI 子命令提供对应方法。

use std::path::Path;

use anyhow::{Context, bail};
use shiroha_client::{ControlClient, EventQuery, JobDetails, JobEvent};
use tokio::time::{Duration, sleep};

use crate::{event_presenter, flow_presenter, job_presenter};

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

        flow_presenter::render_deploy_result(
            &resp.flow_id,
            &resp.version,
            flow_details.as_ref(),
            &resp.warnings,
            json_output,
        )
    }

    pub async fn list_flows(&mut self, json_output: bool) -> anyhow::Result<()> {
        let flows = self.api.list_flows().await?;
        flow_presenter::render_flow_list(&flows, "no flows", json_output)
    }

    pub async fn get_flow(
        &mut self,
        flow_id: &str,
        version: Option<&str>,
        summary: bool,
        json_output: bool,
    ) -> anyhow::Result<()> {
        let flow = self.api.get_flow(flow_id, version).await?;
        flow_presenter::render_flow_details(&flow, summary, json_output)
    }

    pub async fn list_flow_versions(
        &mut self,
        flow_id: &str,
        json_output: bool,
    ) -> anyhow::Result<()> {
        let flows = self.api.list_flow_versions(flow_id).await?;
        flow_presenter::render_flow_list(&flows, "no historical versions", json_output)
    }

    pub async fn delete_flow(
        &mut self,
        flow_id: &str,
        force: bool,
        json_output: bool,
    ) -> anyhow::Result<()> {
        if force {
            let result = self.api.force_delete_flow(flow_id).await?;
            return flow_presenter::render_force_delete_flow(&result, json_output);
        }

        let resp = self.api.delete_flow(flow_id).await?;
        flow_presenter::render_delete_flow(&resp.flow_id, json_output)
    }

    pub async fn create_job(
        &mut self,
        flow_id: &str,
        context: Option<Vec<u8>>,
        json_output: bool,
    ) -> anyhow::Result<()> {
        let resp = self.api.create_job(flow_id, context.clone()).await?;
        job_presenter::render_create_job_result(
            &resp.job_id,
            flow_id,
            context.as_deref(),
            json_output,
        )
    }

    pub async fn get_job(&mut self, job_id: &str, json_output: bool) -> anyhow::Result<()> {
        let job = self.api.get_job(job_id).await?;
        job_presenter::render_job(&job, json_output)
    }

    pub async fn delete_job(
        &mut self,
        job_id: &str,
        force: bool,
        json_output: bool,
    ) -> anyhow::Result<()> {
        if force {
            let result = self.api.force_delete_job(job_id).await?;
            return job_presenter::render_force_delete_job(&result, json_output);
        }

        let resp = self.api.delete_job(job_id).await?;
        job_presenter::render_delete_job(&resp.job_id, json_output)
    }

    pub async fn list_jobs(
        &mut self,
        flow_id: Option<&str>,
        all: bool,
        json_output: bool,
    ) -> anyhow::Result<()> {
        let flow_id = flow_id_for_job_list(flow_id, all)?;
        let jobs = if all {
            self.api.list_all_jobs().await?
        } else {
            self.api
                .list_jobs_for_flow(flow_id.expect("validated flow id"))
                .await?
        };
        job_presenter::render_jobs(&jobs, json_output)
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
        job_presenter::render_trigger_event(job_id, event, payload.as_deref(), json_output)
    }

    pub async fn pause_job(&mut self, job_id: &str, json_output: bool) -> anyhow::Result<()> {
        self.api.pause_job(job_id).await?;
        job_presenter::render_job_operation(job_id, "pause", json_output)
    }

    pub async fn resume_job(&mut self, job_id: &str, json_output: bool) -> anyhow::Result<()> {
        self.api.resume_job(job_id).await?;
        job_presenter::render_job_operation(job_id, "resume", json_output)
    }

    pub async fn cancel_job(&mut self, job_id: &str, json_output: bool) -> anyhow::Result<()> {
        self.api.cancel_job(job_id).await?;
        job_presenter::render_job_operation(job_id, "cancel", json_output)
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
            return event_presenter::render_empty_events(options.json_output);
        }

        event_presenter::render_events(&events, options.pretty, options.json_output)
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
                    return Ok::<JobDetails, anyhow::Error>(job);
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

        job_presenter::render_job(&job, json_output)
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
                event_presenter::render_events(&new_events, options.pretty, options.json_output)?;
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

fn job_matches_target(job: &JobDetails, target_state: Option<&str>) -> bool {
    match target_state {
        Some(target) => job.state == target || job.current_state == target,
        None => matches!(job.state.as_str(), "completed" | "cancelled"),
    }
}

fn apply_tail(mut events: Vec<JobEvent>, tail: Option<usize>) -> Vec<JobEvent> {
    if let Some(tail) = tail
        && events.len() > tail
    {
        events = events.split_off(events.len() - tail);
    }
    events
}

fn flow_id_for_job_list(flow_id: Option<&str>, all: bool) -> anyhow::Result<Option<&str>> {
    if all {
        return Ok(None);
    }

    flow_id
        .map(Some)
        .ok_or_else(|| anyhow::anyhow!("`--flow-id` is required unless `--all` is set"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn job_matches_target_defaults_to_terminal_states() {
        let completed = JobDetails {
            job_id: "job-1".into(),
            flow_id: "flow".into(),
            state: "completed".into(),
            current_state: "approved".into(),
            flow_version: "version-1".into(),
            context_bytes: None,
        };
        let cancelled = JobDetails {
            job_id: "job-2".into(),
            flow_id: "flow".into(),
            state: "cancelled".into(),
            current_state: "idle".into(),
            flow_version: "version-1".into(),
            context_bytes: Some(0),
        };
        let running = JobDetails {
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
    fn apply_tail_trims_latest_events() {
        let selected = apply_tail(
            vec![
                JobEvent {
                    id: "event-1".into(),
                    job_id: "job-1".into(),
                    timestamp_ms: 1,
                    kind: json!({"type": "created"}),
                },
                JobEvent {
                    id: "event-2".into(),
                    job_id: "job-1".into(),
                    timestamp_ms: 2,
                    kind: json!({"type": "transition"}),
                },
                JobEvent {
                    id: "event-3".into(),
                    job_id: "job-1".into(),
                    timestamp_ms: 3,
                    kind: json!({"type": "transition"}),
                },
            ],
            Some(1),
        );

        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "event-3");
    }

    #[test]
    fn flow_id_for_job_list_accepts_all_without_flow_id() {
        assert_eq!(flow_id_for_job_list(None, true).unwrap(), None);
    }

    #[test]
    fn flow_id_for_job_list_requires_flow_id_when_not_listing_all() {
        let error =
            flow_id_for_job_list(None, false).expect_err("missing flow id should return an error");

        assert!(
            error
                .to_string()
                .contains("`--flow-id` is required unless `--all` is set")
        );
    }

    #[test]
    fn flow_id_for_job_list_returns_provided_flow_id() {
        assert_eq!(
            flow_id_for_job_list(Some("flow-a"), false).unwrap(),
            Some("flow-a")
        );
    }
}
