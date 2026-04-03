//! sctl — Shiroha CLI 管理工具
//!
//! 通过 gRPC 连接 shirohad，提供 Flow 部署和 Job 管理的命令行操作。

mod client;

use clap::{Args, Parser};
use tracing_subscriber::EnvFilter;

shadow_rs::shadow!(build);

#[derive(Parser)]
#[command(name = "sctl", version = build::CLAP_LONG_VERSION, about = "Shiroha CLI 管理工具")]
struct Cli {
    /// shirohad 地址
    #[arg(short, long, default_value = "http://[::1]:50051")]
    server: String,

    /// 以 JSON 输出结果，便于脚本消费
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Args)]
struct ContextArgs {
    /// 以 UTF-8 文本传入 Job context
    #[arg(long, conflicts_with_all = ["context_hex", "context_file"])]
    context_text: Option<String>,

    /// 以十六进制字符串传入 Job context
    #[arg(long, value_name = "HEX", conflicts_with_all = ["context_text", "context_file"])]
    context_hex: Option<String>,

    /// 从文件读取原始字节作为 Job context
    #[arg(long, value_name = "PATH", conflicts_with_all = ["context_text", "context_hex"])]
    context_file: Option<String>,
}

#[derive(Args)]
struct PayloadArgs {
    /// 以 UTF-8 文本传入事件 payload
    #[arg(long, conflicts_with_all = ["payload_hex", "payload_file"])]
    payload_text: Option<String>,

    /// 以十六进制字符串传入事件 payload
    #[arg(long, value_name = "HEX", conflicts_with_all = ["payload_text", "payload_file"])]
    payload_hex: Option<String>,

    /// 从文件读取原始字节作为事件 payload
    #[arg(long, value_name = "PATH", conflicts_with_all = ["payload_text", "payload_hex"])]
    payload_file: Option<String>,
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
    /// 查询单个 Flow 详情和 manifest
    Flow {
        #[arg(short = 'i', long)]
        flow_id: String,
    },
    /// 创建 Job
    Create {
        #[arg(short = 'i', long)]
        flow_id: String,
        #[command(flatten)]
        context: ContextArgs,
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
        #[command(flatten)]
        payload: PayloadArgs,
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
        /// 以缩进 JSON 格式打印事件 kind
        #[arg(long)]
        pretty: bool,
        /// 持续轮询并打印新事件
        #[arg(long)]
        follow: bool,
        /// follow 模式下的轮询间隔（毫秒）；配合 --json 时每批新事件输出一个 JSON 数组
        #[arg(long, default_value_t = 500)]
        interval_ms: u64,
    },
    /// 等待 Job 到达目标状态，未指定时等待到终态
    Wait {
        #[arg(short = 'i', long)]
        job_id: String,
        /// 目标 lifecycle state 或 current_state，未指定时等待 completed/cancelled
        #[arg(long)]
        state: Option<String>,
        /// 最大等待时间（毫秒），未指定则一直等待
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// 轮询间隔（毫秒）
        #[arg(long, default_value_t = 500)]
        interval_ms: u64,
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
        Commands::Deploy { file, flow_id } => c.deploy(&flow_id, &file, cli.json).await?,
        Commands::Flows => c.list_flows(cli.json).await?,
        Commands::Flow { flow_id } => c.get_flow(&flow_id, cli.json).await?,
        Commands::Create { flow_id, context } => {
            c.create_job(
                &flow_id,
                client::decode_optional_bytes(
                    context.context_text.as_deref(),
                    context.context_hex.as_deref(),
                    context.context_file.as_deref(),
                )?,
                cli.json,
            )
            .await?
        }
        Commands::Get { job_id } => c.get_job(&job_id, cli.json).await?,
        Commands::Jobs { flow_id } => c.list_jobs(&flow_id, cli.json).await?,
        Commands::Trigger {
            job_id,
            event,
            payload,
        } => {
            c.trigger_event(
                &job_id,
                &event,
                client::decode_optional_bytes(
                    payload.payload_text.as_deref(),
                    payload.payload_hex.as_deref(),
                    payload.payload_file.as_deref(),
                )?,
                cli.json,
            )
            .await?
        }
        Commands::Pause { job_id } => c.pause_job(&job_id, cli.json).await?,
        Commands::Resume { job_id } => c.resume_job(&job_id, cli.json).await?,
        Commands::Cancel { job_id } => c.cancel_job(&job_id, cli.json).await?,
        Commands::Events {
            job_id,
            pretty,
            follow,
            interval_ms,
        } => {
            c.get_job_events(&job_id, pretty, follow, interval_ms, cli.json)
                .await?
        }
        Commands::Wait {
            job_id,
            state,
            timeout_ms,
            interval_ms,
        } => {
            c.wait_job(&job_id, state.as_deref(), timeout_ms, interval_ms, cli.json)
                .await?
        }
    }

    Ok(())
}
