//! sctl — Shiroha CLI 管理工具
//!
//! 通过 gRPC 连接 shirohad，提供 Flow 部署和 Job 管理的命令行操作。

mod client;
mod completion;

use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Args, CommandFactory, Parser, ValueHint};
use clap_complete::env::CompleteEnv;
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
    #[arg(
        long,
        value_name = "PATH",
        value_hint = ValueHint::FilePath,
        conflicts_with_all = ["context_text", "context_hex"]
    )]
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
    #[arg(
        long,
        value_name = "PATH",
        value_hint = ValueHint::FilePath,
        conflicts_with_all = ["payload_text", "payload_hex"]
    )]
    payload_file: Option<String>,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug, Eq, PartialEq)]
enum CompletionShell {
    Bash,
    Elvish,
    Fish,
    PowerShell,
    Zsh,
}

impl CompletionShell {
    fn env_name(self) -> &'static str {
        match self {
            Self::Bash => "bash",
            Self::Elvish => "elvish",
            Self::Fish => "fish",
            Self::PowerShell => "powershell",
            Self::Zsh => "zsh",
        }
    }

    fn default_output_path(self, home: &Path) -> Option<PathBuf> {
        match self {
            Self::Bash => Some(home.join(".local/share/bash-completion/completions/sctl")),
            Self::Elvish => Some(home.join(".config/elvish/lib/sctl.elv")),
            Self::Fish => Some(home.join(".config/fish/completions/sctl.fish")),
            Self::PowerShell => None,
            Self::Zsh => Some(home.join(".zfunc/_sctl")),
        }
    }
}

impl std::fmt::Display for CompletionShell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.env_name())
    }
}

#[derive(Args)]
struct CompleteArgs {
    /// 目标 shell；未指定时尝试从 $SHELL 自动探测
    #[arg(value_enum)]
    shell: Option<CompletionShell>,

    /// 将补全脚本写入 shell 的默认补全目录
    #[arg(long)]
    install: bool,

    /// 将补全脚本写到指定路径
    #[arg(long, value_name = "PATH", value_hint = ValueHint::FilePath)]
    output: Option<PathBuf>,

    /// 打印默认安装路径
    #[arg(long, conflicts_with_all = ["install", "output"])]
    print_path: bool,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// 部署 Flow（上传 WASM 文件）
    Deploy {
        #[arg(short, long, value_hint = ValueHint::FilePath)]
        file: String,
        #[arg(
            short = 'i',
            long,
            add = clap_complete::engine::ArgValueCompleter::new(completion::flow_id_completer)
        )]
        flow_id: String,
    },
    /// 列出所有 Flow
    Flows,
    /// 查询单个 Flow 详情和 manifest
    Flow {
        #[arg(
            short = 'i',
            long,
            add = clap_complete::engine::ArgValueCompleter::new(completion::flow_id_completer)
        )]
        flow_id: String,
        /// 以拓扑摘要视图展示状态、转移和 action
        #[arg(long, conflicts_with = "json")]
        summary: bool,
    },
    /// 删除 Flow；要求不存在关联 Job
    DeleteFlow {
        #[arg(
            short = 'i',
            long,
            add = clap_complete::engine::ArgValueCompleter::new(completion::flow_id_completer)
        )]
        flow_id: String,
    },
    /// 输出 shell 补全脚本
    Complete(CompleteArgs),
    /// 创建 Job
    Create {
        #[arg(
            short = 'i',
            long,
            add = clap_complete::engine::ArgValueCompleter::new(completion::flow_id_completer)
        )]
        flow_id: String,
        #[command(flatten)]
        context: ContextArgs,
    },
    /// 查询 Job 详情
    Get {
        #[arg(
            short = 'i',
            long,
            add = clap_complete::engine::ArgValueCompleter::new(completion::job_id_completer)
        )]
        job_id: String,
    },
    /// 删除 Job；要求 Job 已 cancelled/completed
    DeleteJob {
        #[arg(
            short = 'i',
            long,
            add = clap_complete::engine::ArgValueCompleter::new(completion::job_id_completer)
        )]
        job_id: String,
    },
    /// 列出 Flow 的所有 Job
    Jobs {
        /// 聚合列出所有 Flow 下的 Job
        #[arg(long, conflicts_with = "flow_id")]
        all: bool,
        #[arg(
            short = 'i',
            long,
            required_unless_present = "all",
            add = clap_complete::engine::ArgValueCompleter::new(completion::flow_id_completer)
        )]
        flow_id: Option<String>,
    },
    /// 触发事件
    Trigger {
        #[arg(
            short = 'i',
            long,
            add = clap_complete::engine::ArgValueCompleter::new(completion::job_id_completer)
        )]
        job_id: String,
        #[arg(
            short,
            long,
            add = clap_complete::engine::ArgValueCompleter::new(completion::job_event_completer)
        )]
        event: String,
        #[command(flatten)]
        payload: PayloadArgs,
    },
    /// 暂停 Job
    Pause {
        #[arg(
            short = 'i',
            long,
            add = clap_complete::engine::ArgValueCompleter::new(completion::job_id_completer)
        )]
        job_id: String,
    },
    /// 恢复 Job
    Resume {
        #[arg(
            short = 'i',
            long,
            add = clap_complete::engine::ArgValueCompleter::new(completion::job_id_completer)
        )]
        job_id: String,
    },
    /// 取消 Job
    Cancel {
        #[arg(
            short = 'i',
            long,
            add = clap_complete::engine::ArgValueCompleter::new(completion::job_id_completer)
        )]
        job_id: String,
    },
    /// 查看 Job 事件日志
    Events {
        #[arg(
            short = 'i',
            long,
            add = clap_complete::engine::ArgValueCompleter::new(completion::job_id_completer)
        )]
        job_id: String,
        /// 以缩进 JSON 格式打印事件 kind
        #[arg(long)]
        pretty: bool,
        /// 持续轮询并打印新事件
        #[arg(long)]
        follow: bool,
        /// 仅输出指定类型的事件；可重复传入多个值
        #[arg(
            long = "kind",
            value_name = "TYPE",
            add = clap_complete::engine::ArgValueCompleter::new(completion::event_kind_completer)
        )]
        kind: Vec<String>,
        /// 仅输出最后 N 条事件；follow 模式下首批历史事件也会应用该限制
        #[arg(long, value_name = "N", value_parser = parse_positive_usize)]
        tail: Option<usize>,
        /// follow 模式下的轮询间隔（毫秒）；配合 --json 时每批新事件输出一个 JSON 数组
        #[arg(long, default_value_t = 500)]
        interval_ms: u64,
    },
    /// 等待 Job 到达目标状态，未指定时等待到终态
    Wait {
        #[arg(
            short = 'i',
            long,
            add = clap_complete::engine::ArgValueCompleter::new(completion::job_id_completer)
        )]
        job_id: String,
        /// 目标 lifecycle state 或 current_state，未指定时等待 completed/cancelled
        #[arg(
            long,
            add = clap_complete::engine::ArgValueCompleter::new(completion::wait_state_completer)
        )]
        state: Option<String>,
        /// 最大等待时间（毫秒），未指定则一直等待
        #[arg(long)]
        timeout_ms: Option<u64>,
        /// 轮询间隔（毫秒）
        #[arg(long, default_value_t = 500)]
        interval_ms: u64,
    },
}

fn main() -> anyhow::Result<()> {
    CompleteEnv::with_factory(Cli::command).complete();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async_main())
}

async fn async_main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if let Commands::Complete(args) = cli.command {
        handle_complete(args)?;
        return Ok(());
    }

    // 整个 CLI 生命周期只建立一次 channel，然后把子命令分派给薄封装客户端。
    let mut c = client::ShirohaClient::connect(&cli.server).await?;

    match cli.command {
        Commands::Deploy { file, flow_id } => c.deploy(&flow_id, &file, cli.json).await?,
        Commands::Flows => c.list_flows(cli.json).await?,
        Commands::Flow { flow_id, summary } => c.get_flow(&flow_id, summary, cli.json).await?,
        Commands::DeleteFlow { flow_id } => c.delete_flow(&flow_id, cli.json).await?,
        Commands::Complete(..) => unreachable!("complete command handled before gRPC connect"),
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
        Commands::DeleteJob { job_id } => c.delete_job(&job_id, cli.json).await?,
        Commands::Jobs { all, flow_id } => c.list_jobs(flow_id.as_deref(), all, cli.json).await?,
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
            kind,
            tail,
            interval_ms,
        } => {
            c.get_job_events(
                &job_id,
                client::EventQueryOptions {
                    pretty,
                    follow,
                    kind_filters: kind,
                    tail,
                    interval_ms,
                    json_output: cli.json,
                },
            )
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

fn handle_complete(args: CompleteArgs) -> anyhow::Result<()> {
    let shell = resolve_completion_shell(args.shell)?;
    if args.print_path {
        let path = default_completion_path(shell)?;
        println!("{}", path.display());
        return Ok(());
    }

    let script = generate_completion_script(shell)?;
    if let Some(path) = args.output.as_deref() {
        write_completion_script(path, &script)?;
        println!("{}", path.display());
        return Ok(());
    }
    if args.install {
        let path = default_completion_path(shell)?;
        write_completion_script(&path, &script)?;
        println!("{}", path.display());
        return Ok(());
    }

    std::io::stdout().write_all(&script)?;
    Ok(())
}

fn resolve_completion_shell(shell: Option<CompletionShell>) -> anyhow::Result<CompletionShell> {
    shell.or_else(detect_shell_from_env).ok_or_else(|| {
        anyhow::anyhow!(
            "failed to detect shell from $SHELL; pass one explicitly, e.g. `sctl complete fish`"
        )
    })
}

fn detect_shell_from_env() -> Option<CompletionShell> {
    let shell = std::env::var_os("SHELL")?;
    let shell = Path::new(&shell).file_name()?.to_str()?;
    match shell {
        "bash" => Some(CompletionShell::Bash),
        "elvish" => Some(CompletionShell::Elvish),
        "fish" => Some(CompletionShell::Fish),
        "pwsh" | "powershell" => Some(CompletionShell::PowerShell),
        "zsh" => Some(CompletionShell::Zsh),
        _ => None,
    }
}

fn default_completion_path(shell: CompletionShell) -> anyhow::Result<PathBuf> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("$HOME is not set"))?;
    shell.default_output_path(&home).ok_or_else(|| {
        anyhow::anyhow!("shell `{shell}` has no default install path; use `--output <PATH>`")
    })
}

fn generate_completion_script(shell: CompletionShell) -> anyhow::Result<Vec<u8>> {
    let output = std::process::Command::new(std::env::current_exe()?)
        .env("COMPLETE", shell.env_name())
        .output()?;
    if output.status.success() {
        return Ok(output.stdout);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!(
        "failed to generate {} completion script: {}",
        shell,
        stderr.trim()
    )
}

fn write_completion_script(path: &Path, script: &[u8]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, script)?;
    Ok(())
}

fn parse_positive_usize(input: &str) -> Result<usize, String> {
    let value = input
        .parse::<usize>()
        .map_err(|error| format!("invalid positive integer `{input}`: {error}"))?;
    if value == 0 {
        return Err("value must be greater than 0".to_string());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_shell_from_path_basename() {
        assert_eq!(
            Path::new("/usr/bin/fish")
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| match name {
                    "fish" => Some(CompletionShell::Fish),
                    _ => None,
                }),
            Some(CompletionShell::Fish)
        );
    }

    #[test]
    fn default_output_path_matches_shell_convention() {
        let home = Path::new("/tmp/demo-home");
        assert_eq!(
            CompletionShell::Fish.default_output_path(home),
            Some(home.join(".config/fish/completions/sctl.fish"))
        );
        assert_eq!(
            CompletionShell::Bash.default_output_path(home),
            Some(home.join(".local/share/bash-completion/completions/sctl"))
        );
        assert_eq!(CompletionShell::PowerShell.default_output_path(home), None);
    }
}
