//! gRPC 客户端封装
//!
//! 封装 FlowServiceClient 和 JobServiceClient，为每个 CLI 子命令提供对应方法。

use shiroha_proto::shiroha_api::flow_service_client::FlowServiceClient;
use shiroha_proto::shiroha_api::job_service_client::JobServiceClient;
use shiroha_proto::shiroha_api::*;
use tonic::transport::Channel;

/// shirohad gRPC 客户端
pub struct ShirohaClient {
    flow: FlowServiceClient<Channel>,
    job: JobServiceClient<Channel>,
}

impl ShirohaClient {
    /// 连接到 shirohad gRPC 服务
    pub async fn connect(addr: &str) -> anyhow::Result<Self> {
        let channel = Channel::from_shared(addr.to_string())?.connect().await?;
        Ok(Self {
            flow: FlowServiceClient::new(channel.clone()),
            job: JobServiceClient::new(channel),
        })
    }

    /// 部署 Flow：读取本地 WASM 文件并上传
    pub async fn deploy(&mut self, flow_id: &str, file: &str) -> anyhow::Result<()> {
        let wasm_bytes = std::fs::read(file)?;
        let resp = self
            .flow
            .deploy_flow(DeployFlowRequest {
                flow_id: flow_id.to_string(),
                wasm_bytes,
            })
            .await?
            .into_inner();
        println!("deployed flow_id={} version={}", resp.flow_id, resp.version);
        Ok(())
    }

    pub async fn list_flows(&mut self) -> anyhow::Result<()> {
        let resp = self
            .flow
            .list_flows(ListFlowsRequest {})
            .await?
            .into_inner();
        if resp.flows.is_empty() {
            println!("no flows");
            return Ok(());
        }
        println!(
            "{:<20} {:<38} {:<15} STATES",
            "FLOW_ID", "VERSION", "INITIAL"
        );
        for f in &resp.flows {
            println!(
                "{:<20} {:<38} {:<15} {}",
                f.flow_id, f.version, f.initial_state, f.state_count
            );
        }
        Ok(())
    }

    pub async fn create_job(&mut self, flow_id: &str) -> anyhow::Result<()> {
        let resp = self
            .job
            .create_job(CreateJobRequest {
                flow_id: flow_id.to_string(),
                context: None,
            })
            .await?
            .into_inner();
        println!("created job_id={}", resp.job_id);
        Ok(())
    }

    pub async fn get_job(&mut self, job_id: &str) -> anyhow::Result<()> {
        let resp = self
            .job
            .get_job(GetJobRequest {
                job_id: job_id.to_string(),
            })
            .await?
            .into_inner();
        println!("job_id:        {}", resp.job_id);
        println!("flow_id:       {}", resp.flow_id);
        println!("state:         {}", resp.state);
        println!("current_state: {}", resp.current_state);
        Ok(())
    }

    pub async fn list_jobs(&mut self, flow_id: &str) -> anyhow::Result<()> {
        let resp = self
            .job
            .list_jobs(ListJobsRequest {
                flow_id: flow_id.to_string(),
            })
            .await?
            .into_inner();
        if resp.jobs.is_empty() {
            println!("no jobs");
            return Ok(());
        }
        println!("{:<38} {:<20} {:<12} CURRENT", "JOB_ID", "FLOW_ID", "STATE");
        for j in &resp.jobs {
            println!(
                "{:<38} {:<20} {:<12} {}",
                j.job_id, j.flow_id, j.state, j.current_state
            );
        }
        Ok(())
    }

    pub async fn trigger_event(&mut self, job_id: &str, event: &str) -> anyhow::Result<()> {
        self.job
            .trigger_event(TriggerEventRequest {
                job_id: job_id.to_string(),
                event: event.to_string(),
                payload: None,
            })
            .await?;
        println!("event `{event}` triggered on job {job_id}");
        Ok(())
    }

    pub async fn pause_job(&mut self, job_id: &str) -> anyhow::Result<()> {
        self.job
            .pause_job(PauseJobRequest {
                job_id: job_id.to_string(),
            })
            .await?;
        println!("job {job_id} paused");
        Ok(())
    }

    pub async fn resume_job(&mut self, job_id: &str) -> anyhow::Result<()> {
        self.job
            .resume_job(ResumeJobRequest {
                job_id: job_id.to_string(),
            })
            .await?;
        println!("job {job_id} resumed");
        Ok(())
    }

    pub async fn cancel_job(&mut self, job_id: &str) -> anyhow::Result<()> {
        self.job
            .cancel_job(CancelJobRequest {
                job_id: job_id.to_string(),
            })
            .await?;
        println!("job {job_id} cancelled");
        Ok(())
    }

    pub async fn get_job_events(&mut self, job_id: &str) -> anyhow::Result<()> {
        let resp = self
            .job
            .get_job_events(GetJobEventsRequest {
                job_id: job_id.to_string(),
            })
            .await?
            .into_inner();
        if resp.events.is_empty() {
            println!("no events");
            return Ok(());
        }
        println!("{:<38} {:<16} KIND", "ID", "TIMESTAMP_MS");
        for e in &resp.events {
            println!("{:<38} {:<16} {}", e.id, e.timestamp_ms, e.kind_json);
        }
        Ok(())
    }
}
