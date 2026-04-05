use shiroha_proto::shiroha_api::*;

use crate::client::ControlClient;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForceDeleteFlowResult {
    pub flow_id: String,
    pub deleted_jobs: Vec<crate::job::ForceDeleteJobResult>,
}

impl ControlClient {
    pub async fn deploy_flow(
        &mut self,
        flow_id: &str,
        wasm_bytes: Vec<u8>,
    ) -> anyhow::Result<DeployFlowResponse> {
        Ok(self
            .flow
            .deploy_flow(DeployFlowRequest {
                flow_id: flow_id.to_string(),
                wasm_bytes,
            })
            .await?
            .into_inner())
    }

    pub async fn list_flows(&mut self) -> anyhow::Result<Vec<FlowSummary>> {
        let mut flows = self
            .flow
            .list_flows(ListFlowsRequest {})
            .await?
            .into_inner()
            .flows;
        flows.sort_by(|left, right| left.flow_id.cmp(&right.flow_id));
        Ok(flows)
    }

    pub async fn list_flow_ids(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(self
            .list_flows()
            .await?
            .into_iter()
            .map(|flow| flow.flow_id)
            .collect())
    }

    pub async fn list_flow_versions(&mut self, flow_id: &str) -> anyhow::Result<Vec<FlowSummary>> {
        let mut flows = self
            .flow
            .list_flow_versions(ListFlowVersionsRequest {
                flow_id: flow_id.to_string(),
            })
            .await?
            .into_inner()
            .flows;
        flows.sort_by(|left, right| right.version.cmp(&left.version));
        Ok(flows)
    }

    pub async fn get_flow(
        &mut self,
        flow_id: &str,
        version: Option<&str>,
    ) -> anyhow::Result<GetFlowResponse> {
        Ok(self
            .flow
            .get_flow(GetFlowRequest {
                flow_id: flow_id.to_string(),
                version: version.map(ToString::to_string),
            })
            .await?
            .into_inner())
    }

    pub async fn delete_flow(&mut self, flow_id: &str) -> anyhow::Result<DeleteFlowResponse> {
        Ok(self
            .flow
            .delete_flow(DeleteFlowRequest {
                flow_id: flow_id.to_string(),
            })
            .await?
            .into_inner())
    }

    pub async fn force_delete_flow(
        &mut self,
        flow_id: &str,
    ) -> anyhow::Result<ForceDeleteFlowResult> {
        let jobs = self.list_jobs_for_flow(flow_id).await?;
        let mut deleted_jobs = Vec::with_capacity(jobs.len());
        for job in jobs {
            deleted_jobs.push(self.force_delete_job(&job.job_id).await?);
        }
        self.delete_flow(flow_id).await?;
        Ok(ForceDeleteFlowResult {
            flow_id: flow_id.to_string(),
            deleted_jobs,
        })
    }
}
