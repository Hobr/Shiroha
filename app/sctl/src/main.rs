//! sctl — Shiroha CLI 管理工具
//!
//! 通过 gRPC 连接 shirohad，提供 Flow 部署和 Job 管理的命令行操作。

mod cli_support;
mod client;
mod command_runner;
mod completion;
mod event_presenter;
mod flow_presenter;
mod job_presenter;
mod presenter_support;

use std::path::{Path, PathBuf};

use clap::{Args, Parser, ValueHint};

use crate::cli_support::{parse_positive_u32, parse_positive_u64, parse_positive_usize};

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

#[derive(Args)]
struct FlowIdArgs {
    #[arg(
        short = 'i',
        long,
        add = clap_complete::engine::ArgValueCompleter::new(completion::flow_id_completer)
    )]
    flow_id: String,
}

#[derive(Args)]
struct JobIdArgs {
    #[arg(
        short = 'i',
        long,
        add = clap_complete::engine::ArgValueCompleter::new(completion::job_id_completer)
    )]
    job_id: String,
}

#[derive(Args)]
struct FlowDeployArgs {
    #[arg(short, long, value_hint = ValueHint::FilePath)]
    file: String,
    #[command(flatten)]
    flow: FlowIdArgs,
}

#[derive(Args)]
struct FlowRmArgs {
    #[command(flatten)]
    flow: FlowIdArgs,
    /// 自动删除关联 Job，并在需要时先取消运行中的 Job
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
struct FlowGetArgs {
    #[command(flatten)]
    flow: FlowIdArgs,
    /// 查询指定部署版本；默认返回 latest alias
    #[arg(long, value_name = "VERSION")]
    version: Option<String>,
    /// 以拓扑摘要视图展示状态、转移和 action
    #[arg(long, conflicts_with = "json")]
    summary: bool,
}

#[derive(Args)]
struct JobCreateArgs {
    #[command(flatten)]
    flow: FlowIdArgs,
    #[command(flatten)]
    context: ContextArgs,
    #[arg(long, value_name = "MS", value_parser = parse_positive_u64)]
    max_lifetime_ms: Option<u64>,
}

#[derive(Args)]
struct JobRmArgs {
    #[command(flatten)]
    job: JobIdArgs,
    /// 若 Job 仍在运行或暂停，先自动取消再删除
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
struct JobsListArgs {
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
}

#[derive(Args)]
struct JobTriggerArgs {
    #[command(flatten)]
    job: JobIdArgs,
    #[arg(
        short,
        long,
        add = clap_complete::engine::ArgValueCompleter::new(completion::job_event_completer)
    )]
    event: String,
    #[command(flatten)]
    payload: PayloadArgs,
}

#[derive(Args)]
struct JobEventsArgs {
    #[command(flatten)]
    job: JobIdArgs,
    /// 以缩进 JSON 格式打印事件 kind
    #[arg(long)]
    pretty: bool,
    /// 持续轮询并打印新事件
    #[arg(long)]
    follow: bool,
    /// 仅返回给定事件 ID 之后的新事件
    #[arg(
        long,
        value_name = "EVENT_ID",
        add = clap_complete::engine::ArgValueCompleter::new(completion::job_event_id_completer)
    )]
    since_id: Option<String>,
    /// 仅返回严格晚于该时间戳（毫秒）的事件
    #[arg(long)]
    since_timestamp_ms: Option<u64>,
    /// 服务端最多返回前 N 条匹配事件
    #[arg(long, value_name = "N", value_parser = parse_positive_u32)]
    limit: Option<u32>,
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
}

#[derive(Args)]
struct JobWaitArgs {
    #[command(flatten)]
    job: JobIdArgs,
    /// 目标 lifecycle state，未指定时等待 completed/cancelled
    #[arg(
        long,
        value_name = "STATE",
        conflicts_with = "current_state",
        add = clap_complete::engine::ArgValueCompleter::new(
            completion::wait_lifecycle_state_completer,
        )
    )]
    state: Option<String>,
    /// 目标 current_state
    #[arg(
        long,
        value_name = "STATE",
        conflicts_with = "state",
        add = clap_complete::engine::ArgValueCompleter::new(
            completion::wait_current_state_completer,
        )
    )]
    current_state: Option<String>,
    /// 最大等待时间（毫秒），未指定则一直等待
    #[arg(long)]
    timeout_ms: Option<u64>,
    /// 轮询间隔（毫秒）
    #[arg(long, default_value_t = 500)]
    interval_ms: u64,
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
    /// Flow 管理
    Flow {
        #[command(subcommand)]
        command: FlowCommands,
    },
    /// Job 管理
    Job {
        #[command(subcommand)]
        command: JobCommands,
    },
    /// 输出 shell 补全脚本
    Complete(CompleteArgs),
}

#[derive(clap::Subcommand)]
enum FlowCommands {
    /// 部署 Flow（上传 WASM 文件）
    Deploy(FlowDeployArgs),
    /// 列出所有 Flow
    Ls,
    /// 查询单个 Flow 详情和 manifest
    Get(FlowGetArgs),
    /// 列出某个 Flow 的历史部署版本
    Vers(FlowIdArgs),
    /// 删除 Flow；要求不存在关联 Job
    Rm(FlowRmArgs),
}

#[derive(clap::Subcommand)]
enum JobCommands {
    /// 创建 Job
    New(JobCreateArgs),
    /// 查询 Job 详情
    Get(JobIdArgs),
    /// 删除 Job；要求 Job 已 cancelled/completed
    Rm(JobRmArgs),
    /// 列出 Job
    Ls(JobsListArgs),
    /// 触发事件
    Trig(JobTriggerArgs),
    /// 暂停 Job
    Pause(JobIdArgs),
    /// 恢复 Job
    Resume(JobIdArgs),
    /// 取消 Job
    Cancel(JobIdArgs),
    /// 查看 Job 事件日志
    Logs(JobEventsArgs),
    /// 等待 Job 到达目标状态，未指定时等待到终态
    Wait(JobWaitArgs),
}

fn main() -> anyhow::Result<()> {
    command_runner::run()
}

#[cfg(test)]
mod tests {
    use super::*;

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
