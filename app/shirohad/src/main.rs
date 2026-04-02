//! shirohad — Shiroha 分布式状态机守护进程
//!
//! Phase 1 仅支持 standalone 模式（Controller + Node 同进程）。
//! 启动后通过 gRPC 对外提供 FlowService 和 JobService。

use clap::Parser;
use tracing_subscriber::EnvFilter;

pub mod job_service;

mod flow_service;
#[cfg(test)]
mod grpc_tests;
mod server;
#[cfg(test)]
mod test_support;
shadow_rs::shadow!(build);

#[derive(Parser)]
#[command(name = "shirohad", version = build::CLAP_LONG_VERSION, about = "Shiroha 分布式状态机守护进程")]
struct Cli {
    /// 运行模式：standalone / controller / node
    #[arg(short, long, default_value = "standalone")]
    mode: String,

    /// gRPC 监听地址
    #[arg(long, default_value = "[::1]:50051")]
    listen: String,

    /// 数据目录（存放 redb 数据库文件）
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

    // Phase 1 的不同 mode 先共享同一套启动路径；后续再按 mode 分化控制面/执行面职责。
    let srv = server::ShirohaServer::new(&cli.data_dir).await?;
    srv.start(&cli.listen).await?;

    Ok(())
}
