use shiroha_proto::shiroha_api::flow_service_client::FlowServiceClient;
use shiroha_proto::shiroha_api::job_service_client::JobServiceClient;
use tonic::transport::Channel;

pub struct ControlClient {
    pub(crate) flow: FlowServiceClient<Channel>,
    pub(crate) job: JobServiceClient<Channel>,
}

impl ControlClient {
    pub async fn connect(addr: &str) -> anyhow::Result<Self> {
        let channel = Channel::from_shared(addr.to_string())?.connect().await?;
        Ok(Self {
            flow: FlowServiceClient::new(channel.clone()),
            job: JobServiceClient::new(channel),
        })
    }
}
