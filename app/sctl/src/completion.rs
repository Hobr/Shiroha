use std::ffi::{OsStr, OsString};
use std::time::Duration;

use clap_complete::engine::CompletionCandidate;

use crate::client::ShirohaClient;

const DEFAULT_SERVER: &str = "http://[::1]:50051";
const COMPLETION_TIMEOUT: Duration = Duration::from_millis(500);
const LIFECYCLE_STATES: &[&str] = &["running", "paused", "cancelled", "completed"];
const EVENT_KIND_NAMES: &[&str] = &[
    "created",
    "transition",
    "action_complete",
    "paused",
    "resumed",
    "cancelled",
    "completed",
];

pub fn flow_id_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let context = CompletionContext::from_process_args();
    let candidates = run_query(async move {
        let mut client = ShirohaClient::connect(&context.server).await?;
        client.list_flow_ids().await
    })
    .unwrap_or_default();

    filter_candidates(current, candidates)
}

pub fn job_id_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let context = CompletionContext::from_process_args();
    let candidates = run_query(async move {
        let mut client = ShirohaClient::connect(&context.server).await?;
        client.list_job_ids().await
    })
    .unwrap_or_default();

    filter_candidates(current, candidates)
}

pub fn job_event_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let context = CompletionContext::from_process_args();
    let Some(job_id) = context.job_id.clone() else {
        return Vec::new();
    };

    let candidates = run_query(async move {
        let mut client = ShirohaClient::connect(&context.server).await?;
        client.list_job_event_names(&job_id).await
    })
    .unwrap_or_default();

    filter_candidates(current, candidates)
}

pub fn wait_state_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    let context = CompletionContext::from_process_args();
    let job_id = context.job_id.clone();

    let candidates = run_query(async move {
        let mut client = ShirohaClient::connect(&context.server).await?;
        let mut states = LIFECYCLE_STATES
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        if let Some(job_id) = job_id.as_deref() {
            states.extend(client.list_wait_states(job_id).await?);
        }
        sort_dedup(&mut states);
        Ok(states)
    })
    .unwrap_or_default();

    filter_candidates(current, candidates)
}

pub fn event_kind_completer(current: &OsStr) -> Vec<CompletionCandidate> {
    filter_candidates(
        current,
        EVENT_KIND_NAMES
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
    )
}

fn run_query<F>(future: F) -> anyhow::Result<Vec<String>>
where
    F: std::future::Future<Output = anyhow::Result<Vec<String>>>,
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        tokio::time::timeout(COMPLETION_TIMEOUT, future)
            .await
            .map_err(anyhow::Error::from)?
    })
}

fn filter_candidates(current: &OsStr, mut values: Vec<String>) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return Vec::new();
    };
    sort_dedup(&mut values);
    values
        .into_iter()
        .filter(|value| value.starts_with(current))
        .map(CompletionCandidate::new)
        .collect()
}

fn sort_dedup(values: &mut Vec<String>) {
    values.sort_unstable();
    values.dedup();
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CompletionContext {
    server: String,
    command: Option<CompletionCommand>,
    job_id: Option<String>,
}

impl CompletionContext {
    fn from_process_args() -> Self {
        Self::from_args(std::env::args_os())
    }

    fn from_args<I>(args: I) -> Self
    where
        I: IntoIterator<Item = OsString>,
    {
        let args = extract_completion_args(args);
        let mut context = Self {
            server: DEFAULT_SERVER.to_string(),
            command: None,
            job_id: None,
        };
        let mut pending = PendingValue::None;

        for token in args.into_iter().skip(1) {
            let Some(token) = token.to_str() else {
                pending = PendingValue::None;
                continue;
            };

            match pending {
                PendingValue::Server => {
                    context.server = token.to_string();
                    pending = PendingValue::None;
                    continue;
                }
                PendingValue::JobId => {
                    context.job_id = Some(token.to_string());
                    pending = PendingValue::None;
                    continue;
                }
                PendingValue::None => {}
            }

            if let Some(value) = token.strip_prefix("--server=") {
                context.server = value.to_string();
                continue;
            }
            if let Some(value) = token.strip_prefix("--job-id=") {
                context.job_id = Some(value.to_string());
                continue;
            }

            match token {
                "--server" | "-s" => pending = PendingValue::Server,
                "--job-id" => pending = PendingValue::JobId,
                "-i" if context.command.is_some_and(CompletionCommand::uses_job_id) => {
                    pending = PendingValue::JobId
                }
                "--json" => {}
                _ if context.command.is_none() && !token.starts_with('-') => {
                    context.command = CompletionCommand::parse(token);
                }
                _ => pending = PendingValue::None,
            }
        }

        context
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CompletionCommand {
    Get,
    DeleteJob,
    Trigger,
    Pause,
    Resume,
    Cancel,
    Events,
    Wait,
}

impl CompletionCommand {
    fn parse(token: &str) -> Option<Self> {
        match token {
            "get" => Some(Self::Get),
            "delete-job" => Some(Self::DeleteJob),
            "trigger" => Some(Self::Trigger),
            "pause" => Some(Self::Pause),
            "resume" => Some(Self::Resume),
            "cancel" => Some(Self::Cancel),
            "events" => Some(Self::Events),
            "wait" => Some(Self::Wait),
            _ => None,
        }
    }

    fn uses_job_id(self) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingValue {
    None,
    Server,
    JobId,
}

fn extract_completion_args<I>(args: I) -> Vec<OsString>
where
    I: IntoIterator<Item = OsString>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    let Some(escape_index) = args.iter().position(|arg| arg == "--") else {
        return args;
    };
    args.into_iter().skip(escape_index + 1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_completion_args_prefers_words_after_escape() {
        let args = extract_completion_args(vec![
            OsString::from("/tmp/sctl"),
            OsString::from("bash"),
            OsString::from("--"),
            OsString::from("sctl"),
            OsString::from("wait"),
            OsString::from("--job-id"),
            OsString::from("job-1"),
        ]);

        assert_eq!(
            args,
            vec![
                OsString::from("sctl"),
                OsString::from("wait"),
                OsString::from("--job-id"),
                OsString::from("job-1"),
            ]
        );
    }

    #[test]
    fn completion_context_parses_server_and_job_id() {
        let context = CompletionContext::from_args(vec![
            OsString::from("/tmp/sctl"),
            OsString::from("bash"),
            OsString::from("--"),
            OsString::from("sctl"),
            OsString::from("--server=http://127.0.0.1:50051"),
            OsString::from("trigger"),
            OsString::from("--job-id"),
            OsString::from("job-42"),
            OsString::from("--event"),
            OsString::from("ap"),
        ]);

        assert_eq!(context.server, "http://127.0.0.1:50051");
        assert_eq!(context.command, Some(CompletionCommand::Trigger));
        assert_eq!(context.job_id.as_deref(), Some("job-42"));
    }

    #[test]
    fn filter_candidates_matches_prefix() {
        let candidates = filter_candidates(
            OsStr::new("ap"),
            vec!["approve".into(), "archive".into(), "approve".into()],
        );

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].get_value(), OsStr::new("approve"));
    }

    #[test]
    fn event_kind_completer_returns_known_kinds() {
        let candidates = event_kind_completer(OsStr::new("co"));

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].get_value(), OsStr::new("completed"));
    }
}
