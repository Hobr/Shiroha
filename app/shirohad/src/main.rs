mod flow_service;
mod job_service;
mod server;

use clap::Parser;
use tracing_subscriber::EnvFilter;

shadow_rs::shadow!(build);

#[derive(Parser)]
#[command(name = "shirohad", version = build::CLAP_LONG_VERSION, about = "Shiroha 分布式状态机守护进程")]
struct Cli {
    /// 运行模式
    #[arg(short, long, default_value = "standalone")]
    mode: String,

    /// gRPC 监听地址
    #[arg(long, default_value = "[::1]:50051")]
    listen: String,

    /// 数据目录
    #[arg(long, default_value = "./data")]
    data_dir: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    tracing::info!(
        version = build::PKG_VERSION,
        mode = cli.mode,
        listen = cli.listen,
        "starting shirohad"
    );

    let srv = server::ShirohaServer::new(&cli.data_dir)?;
    srv.start(&cli.listen).await?;

    Ok(())
}
