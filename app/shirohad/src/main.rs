//! shirohad — Shiroha 分布式状态机守护进程
//!
//! Phase 1 仅支持 standalone 模式（Controller + Node 同进程）。
//! 启动后通过 gRPC 对外提供 FlowService 和 JobService。

use std::path::PathBuf;

use anyhow::bail;
use clap::Parser;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::writer::MakeWriterExt;

#[cfg(test)]
mod grpc_tests;
#[cfg(test)]
mod test_support;

pub mod job_service;
pub mod node_runtime;

mod flow_registry;
mod flow_service;
mod job_events;
mod job_runtime;
mod server;
mod service_support;
shadow_rs::shadow!(build);

#[derive(clap::ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
enum RunMode {
    Standalone,
    Controller,
    Node,
}

impl RunMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Standalone => "standalone",
            Self::Controller => "controller",
            Self::Node => "node",
        }
    }
}

impl std::fmt::Display for RunMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Parser)]
#[command(name = "shirohad", version = build::CLAP_LONG_VERSION, about = "Shiroha 分布式状态机守护进程")]
struct Cli {
    /// 运行模式：standalone / controller / node
    #[arg(short, long, value_enum, default_value_t = RunMode::Standalone)]
    mode: RunMode,

    /// gRPC 监听地址
    #[arg(long, default_value = "[::1]:50051")]
    listen: String,

    /// 数据目录（存放 redb 数据库文件）
    #[arg(long, default_value = "./data")]
    data_dir: String,
}

const LOG_FILE_BASENAME: &str = "shirohad.log";

fn logs_dir(data_dir: &str) -> PathBuf {
    PathBuf::from(data_dir).join("logs")
}

fn init_tracing(data_dir: &str) -> anyhow::Result<WorkerGuard> {
    let logs_dir = logs_dir(data_dir);
    std::fs::create_dir_all(&logs_dir)?;

    let file_appender = tracing_appender::rolling::daily(&logs_dir, LOG_FILE_BASENAME);
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(false)
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr.and(file_writer))
        .init();

    Ok(guard)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let _log_guard = init_tracing(&cli.data_dir)?;
    let log_dir = logs_dir(&cli.data_dir);

    tracing::info!(
        version = build::PKG_VERSION,
        mode = %cli.mode,
        listen = cli.listen,
        data_dir = cli.data_dir,
        log_dir = %log_dir.display(),
        "starting shirohad"
    );

    if cli.mode != RunMode::Standalone {
        bail!(
            "mode `{}` is not implemented yet; use `--mode standalone`",
            cli.mode
        );
    }

    let srv = server::ShirohaServer::new(&cli.data_dir).await?;
    srv.start(&cli.listen).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_mode_display_matches_cli_values() {
        assert_eq!(RunMode::Standalone.to_string(), "standalone");
        assert_eq!(RunMode::Controller.to_string(), "controller");
        assert_eq!(RunMode::Node.to_string(), "node");
    }

    #[test]
    fn logs_dir_is_nested_under_data_dir() {
        assert_eq!(logs_dir("./data"), PathBuf::from("./data").join("logs"));
    }
}
