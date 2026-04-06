use anyhow::Context;
use serde_json::Value;
use shiroha_proto::shiroha_api::*;

use crate::client::ControlClient;
use crate::job::ForceDeleteJobResult;
use crate::manifest::parse_json_value_required;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowVersionSummary {
    pub flow_id: String,
    pub version: String,
    pub initial_state: String,
    pub state_count: u32,
}

impl From<FlowSummary> for FlowVersionSummary {
    fn from(value: FlowSummary) -> Self {
        Self {
            flow_id: value.flow_id,
            version: value.version,
            initial_state: value.initial_state,
            state_count: value.state_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowDetails {
    pub flow_id: String,
    pub version: String,
    pub host_world: Option<String>,
    pub manifest: Value,
}

impl TryFrom<GetFlowResponse> for FlowDetails {
    type Error = anyhow::Error;

    fn try_from(value: GetFlowResponse) -> Result<Self, Self::Error> {
        let manifest = parse_json_value_required(&value.manifest_json, "manifest_json")
            .context("invalid flow manifest returned by server")?;
        Ok(Self {
            flow_id: value.flow_id,
            version: value.version,
            host_world: manifest_host_world(&manifest),
            manifest,
        })
    }
}

fn manifest_host_world(manifest: &Value) -> Option<String> {
    manifest
        .get("host_world")
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForceDeleteFlowResult {
    pub flow_id: String,
    pub deleted_jobs: Vec<ForceDeleteJobResult>,
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

    pub async fn list_flows(&mut self) -> anyhow::Result<Vec<FlowVersionSummary>> {
        let mut flows = self
            .flow
            .list_flows(ListFlowsRequest {})
            .await?
            .into_inner()
            .flows;
        flows.sort_by(|left, right| left.flow_id.cmp(&right.flow_id));
        Ok(flows.into_iter().map(FlowVersionSummary::from).collect())
    }

    pub async fn list_flow_ids(&mut self) -> anyhow::Result<Vec<String>> {
        Ok(self
            .list_flows()
            .await?
            .into_iter()
            .map(|flow| flow.flow_id)
            .collect())
    }

    pub async fn list_flow_versions(
        &mut self,
        flow_id: &str,
    ) -> anyhow::Result<Vec<FlowVersionSummary>> {
        let mut flows = self
            .flow
            .list_flow_versions(ListFlowVersionsRequest {
                flow_id: flow_id.to_string(),
            })
            .await?
            .into_inner()
            .flows;
        flows.sort_by(|left, right| right.version.cmp(&left.version));
        Ok(flows.into_iter().map(FlowVersionSummary::from).collect())
    }

    pub async fn get_flow(
        &mut self,
        flow_id: &str,
        version: Option<&str>,
    ) -> anyhow::Result<FlowDetails> {
        let response = self
            .flow
            .get_flow(GetFlowRequest {
                flow_id: flow_id.to_string(),
                version: version.map(ToString::to_string),
            })
            .await?
            .into_inner();

        FlowDetails::try_from(response)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flow_details_parse_manifest_json() {
        let details = FlowDetails::try_from(GetFlowResponse {
            flow_id: "flow-a".into(),
            version: "v1".into(),
            manifest_json: r#"{"host_world":"sandbox","initial_state":"idle"}"#.into(),
        })
        .expect("manifest json should parse");

        assert_eq!(details.flow_id, "flow-a");
        assert_eq!(details.version, "v1");
        assert_eq!(details.host_world.as_deref(), Some("sandbox"));
        assert_eq!(
            details.manifest,
            json!({"host_world": "sandbox", "initial_state": "idle"})
        );
    }
}
