//! shirohad — Shiroha 分布式状态机守护进程
//!
//! Phase 1 仅支持 standalone 模式（Controller + Node 同进程）。
//! 启动后通过 gRPC 对外提供 FlowService 和 JobService。

use anyhow::bail;
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    tracing::info!(
        version = build::PKG_VERSION,
        mode = %cli.mode,
        listen = cli.listen,
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
}
