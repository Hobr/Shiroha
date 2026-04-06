use std::collections::HashSet;

use shiroha_core::event::{EventKind, EventRecord};
use shiroha_proto::shiroha_api::GetJobEventsRequest;
use tonic::Status;
use uuid::Uuid;

use crate::service_support::parse_uuid;

#[derive(Debug)]
pub(crate) struct JobEventsQuery {
    pub(crate) job_id: Uuid,
    pub(crate) job_id_text: String,
    pub(crate) since_id_text: Option<String>,
    pub(crate) since_timestamp_ms: Option<u64>,
    pub(crate) limit: Option<u32>,
    pub(crate) kinds: HashSet<String>,
}

pub(crate) fn validate_query(req: GetJobEventsRequest) -> Result<JobEventsQuery, Status> {
    let job_id = parse_uuid(&req.job_id)?;

    if req.since_id.is_some() && req.since_timestamp_ms.is_some() {
        return Err(Status::invalid_argument(
            "`since_id` and `since_timestamp_ms` cannot be used together",
        ));
    }
    if req.limit == Some(0) {
        return Err(Status::invalid_argument("`limit` must be greater than 0"));
    }
    if let Some(kind) = req.kind.iter().find(|kind| {
        !matches!(
            kind.as_str(),
            "created"
                | "transition"
                | "action_complete"
                | "paused"
                | "resumed"
                | "cancelled"
                | "completed"
        )
    }) {
        return Err(Status::invalid_argument(format!(
            "unknown event kind filter: {kind}"
        )));
    }

    let since_id_text = req.since_id.clone();
    Ok(JobEventsQuery {
        job_id,
        job_id_text: req.job_id,
        since_id_text,
        since_timestamp_ms: req.since_timestamp_ms,
        limit: req.limit,
        kinds: req.kind.into_iter().collect(),
    })
}

pub(crate) fn filter_events(
    mut events: Vec<EventRecord>,
    query: &JobEventsQuery,
) -> Result<Vec<EventRecord>, Status> {
    if let Some(since_id) = query.since_id_text.as_deref() {
        let cursor = parse_uuid(since_id)?;
        let Some(index) = events.iter().position(|event| event.id == cursor) else {
            return Err(Status::invalid_argument(format!(
                "event `{since_id}` not found for job `{}`",
                query.job_id_text
            )));
        };
        events.drain(..=index);
    }
    if let Some(since_timestamp_ms) = query.since_timestamp_ms {
        events.retain(|event| event.timestamp_ms > since_timestamp_ms);
    }
    if !query.kinds.is_empty() {
        events.retain(|event| query.kinds.contains(event_kind_name(&event.kind)));
    }
    if let Some(limit) = query.limit {
        events.truncate(limit as usize);
    }
    Ok(events)
}

pub(crate) fn event_kind_name(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::Created { .. } => "created",
        EventKind::Transition { .. } => "transition",
        EventKind::ActionComplete { .. } => "action_complete",
        EventKind::Paused => "paused",
        EventKind::Resumed => "resumed",
        EventKind::Cancelled => "cancelled",
        EventKind::Completed { .. } => "completed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shiroha_core::job::ExecutionStatus;

    fn sample_events(job_id: Uuid) -> Vec<EventRecord> {
        vec![
            EventRecord {
                id: Uuid::from_u128(1),
                job_id,
                timestamp_ms: 10,
                kind: EventKind::Created {
                    flow_id: "flow".into(),
                    flow_version: Uuid::from_u128(11),
                    initial_state: "idle".into(),
                },
            },
            EventRecord {
                id: Uuid::from_u128(2),
                job_id,
                timestamp_ms: 20,
                kind: EventKind::Transition {
                    event: "approve".into(),
                    from: "idle".into(),
                    to: "done".into(),
                    action: Some("ship".into()),
                },
            },
            EventRecord {
                id: Uuid::from_u128(3),
                job_id,
                timestamp_ms: 30,
                kind: EventKind::ActionComplete {
                    action: "ship".into(),
                    node_id: None,
                    status: ExecutionStatus::Success,
                },
            },
        ]
    }

    #[test]
    fn validate_query_rejects_since_id_and_timestamp_together() {
        let request = GetJobEventsRequest {
            job_id: Uuid::from_u128(100).to_string(),
            since_id: Some(Uuid::from_u128(1).to_string()),
            since_timestamp_ms: Some(20),
            kind: Vec::new(),
            limit: None,
        };

        let error = validate_query(request).expect_err("query should be rejected");
        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert_eq!(
            error.message(),
            "`since_id` and `since_timestamp_ms` cannot be used together"
        );
    }

    #[test]
    fn validate_query_rejects_invalid_job_id_before_other_constraints() {
        let request = GetJobEventsRequest {
            job_id: "not-a-uuid".into(),
            since_id: Some(Uuid::from_u128(1).to_string()),
            since_timestamp_ms: Some(20),
            kind: vec!["unknown".into()],
            limit: Some(0),
        };

        let error = validate_query(request).expect_err("invalid job_id should be rejected first");
        assert_eq!(error.code(), tonic::Code::InvalidArgument);
        assert_eq!(error.message(), "invalid UUID: not-a-uuid");
    }

    #[test]
    fn validate_query_does_not_parse_since_id_before_event_loading() {
        let request = GetJobEventsRequest {
            job_id: Uuid::from_u128(100).to_string(),
            since_id: Some("not-a-uuid".into()),
            since_timestamp_ms: None,
            kind: Vec::new(),
            limit: None,
        };

        let query = validate_query(request).expect("since_id parsing should be deferred");
        assert_eq!(query.since_id_text.as_deref(), Some("not-a-uuid"));
    }

    #[test]
    fn filter_events_applies_cursor_kind_and_limit() {
        let job_id = Uuid::from_u128(100);
        let request = GetJobEventsRequest {
            job_id: job_id.to_string(),
            since_id: Some(Uuid::from_u128(1).to_string()),
            since_timestamp_ms: None,
            kind: vec!["transition".into(), "action_complete".into()],
            limit: Some(1),
        };
        let query = validate_query(request).expect("valid query");

        let filtered = filter_events(sample_events(job_id), &query).expect("filter events");

        assert_eq!(filtered.len(), 1);
        assert!(matches!(filtered[0].kind, EventKind::Transition { .. }));
    }
}
