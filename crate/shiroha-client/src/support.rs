use crate::job::JobDetails;

pub(crate) fn bound_flow_request(job: &JobDetails) -> shiroha_proto::shiroha_api::GetFlowRequest {
    shiroha_proto::shiroha_api::GetFlowRequest {
        flow_id: job.flow_id.clone(),
        version: Some(job.flow_version.clone()),
    }
}

pub(crate) fn sort_jobs(jobs: &mut [shiroha_proto::shiroha_api::GetJobResponse]) {
    jobs.sort_by(|left, right| {
        left.flow_id
            .cmp(&right.flow_id)
            .then_with(|| left.job_id.cmp(&right.job_id))
    });
}

pub(crate) fn sort_job_details(jobs: &mut [JobDetails]) {
    jobs.sort_by(|left, right| {
        left.flow_id
            .cmp(&right.flow_id)
            .then_with(|| left.job_id.cmp(&right.job_id))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_job_details_orders_by_flow_then_job_id() {
        let mut jobs = vec![
            JobDetails {
                job_id: "job-2".into(),
                flow_id: "flow-b".into(),
                state: "running".into(),
                current_state: "s1".into(),
                flow_version: "v1".into(),
                context_bytes: None,
            },
            JobDetails {
                job_id: "job-3".into(),
                flow_id: "flow-a".into(),
                state: "running".into(),
                current_state: "s1".into(),
                flow_version: "v1".into(),
                context_bytes: None,
            },
            JobDetails {
                job_id: "job-1".into(),
                flow_id: "flow-a".into(),
                state: "running".into(),
                current_state: "s1".into(),
                flow_version: "v1".into(),
                context_bytes: None,
            },
        ];

        sort_job_details(&mut jobs);

        let pairs = jobs
            .into_iter()
            .map(|job| (job.flow_id, job.job_id))
            .collect::<Vec<_>>();
        assert_eq!(
            pairs,
            vec![
                ("flow-a".to_string(), "job-1".to_string()),
                ("flow-a".to_string(), "job-3".to_string()),
                ("flow-b".to_string(), "job-2".to_string()),
            ]
        );
    }
}
