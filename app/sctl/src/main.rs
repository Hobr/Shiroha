//! sctl — Shiroha CLI 管理工具
//!
//! 通过 gRPC 连接 shirohad，提供 Flow 部署和 Job 管理的命令行操作。

mod client;

use clap::Parser;
use tracing_subscriber::EnvFilter;

shadow_rs::shadow!(build);

#[derive(Parser)]
#[command(name = "sctl", version = build::CLAP_LONG_VERSION, about = "Shiroha CLI 管理工具")]
struct Cli {
    /// shirohad 地址
    #[arg(short, long, default_value = "http://[::1]:50051")]
    server: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// 部署 Flow（上传 WASM 文件）
    Deploy {
        #[arg(short, long)]
        file: String,
        #[arg(short = 'i', long)]
        flow_id: String,
    },
    /// 列出所有 Flow
    Flows,
    /// 创建 Job
    Create {
        #[arg(short = 'i', long)]
        flow_id: String,
    },
    /// 查询 Job 详情
    Get {
        #[arg(short = 'i', long)]
        job_id: String,
    },
    /// 列出 Flow 的所有 Job
    Jobs {
        #[arg(short = 'i', long)]
        flow_id: String,
    },
    /// 触发事件
    Trigger {
        #[arg(short = 'i', long)]
        job_id: String,
        #[arg(short, long)]
        event: String,
    },
    /// 暂停 Job
    Pause {
        #[arg(short = 'i', long)]
        job_id: String,
    },
    /// 恢复 Job
    Resume {
        #[arg(short = 'i', long)]
        job_id: String,
    },
    /// 取消 Job
    Cancel {
        #[arg(short = 'i', long)]
        job_id: String,
    },
    /// 查看 Job 事件日志
    Events {
        #[arg(short = 'i', long)]
        job_id: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    // 整个 CLI 生命周期只建立一次 channel，然后把子命令分派给薄封装客户端。
    let mut c = client::ShirohaClient::connect(&cli.server).await?;

    match cli.command {
        Commands::Deploy { file, flow_id } => c.deploy(&flow_id, &file).await?,
        Commands::Flows => c.list_flows().await?,
        Commands::Create { flow_id } => c.create_job(&flow_id).await?,
        Commands::Get { job_id } => c.get_job(&job_id).await?,
        Commands::Jobs { flow_id } => c.list_jobs(&flow_id).await?,
        Commands::Trigger { job_id, event } => c.trigger_event(&job_id, &event).await?,
        Commands::Pause { job_id } => c.pause_job(&job_id).await?,
        Commands::Resume { job_id } => c.resume_job(&job_id).await?,
        Commands::Cancel { job_id } => c.cancel_job(&job_id).await?,
        Commands::Events { job_id } => c.get_job_events(&job_id).await?,
    }

    Ok(())
}
