use clap::{CommandFactory, Parser};
use clap_complete::env::CompleteEnv;
use tracing_subscriber::EnvFilter;

use crate::{Cli, Commands, FlowCommands, JobCommands, cli_support, client};

pub(crate) fn run() -> anyhow::Result<()> {
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
        cli_support::handle_complete(args)?;
        return Ok(());
    }

    // 整个 CLI 生命周期只建立一次 channel，然后把子命令分派给薄封装客户端。
    let mut c = client::ShirohaClient::connect(&cli.server).await?;

    match cli.command {
        Commands::Flow { command } => dispatch_flow_command(&mut c, command, cli.json).await?,
        Commands::Job { command } => dispatch_job_command(&mut c, command, cli.json).await?,
        Commands::Complete(..) => unreachable!("complete command handled before gRPC connect"),
    }

    Ok(())
}

async fn dispatch_flow_command(
    client: &mut client::ShirohaClient,
    command: FlowCommands,
    json_output: bool,
) -> anyhow::Result<()> {
    match command {
        FlowCommands::Deploy(args) => {
            client
                .deploy(&args.flow.flow_id, &args.file, json_output)
                .await
        }
        FlowCommands::Ls => client.list_flows(json_output).await,
        FlowCommands::Get(args) => {
            client
                .get_flow(
                    &args.flow.flow_id,
                    args.version.as_deref(),
                    args.summary,
                    json_output,
                )
                .await
        }
        FlowCommands::Vers(args) => client.list_flow_versions(&args.flow_id, json_output).await,
        FlowCommands::Rm(args) => {
            client
                .delete_flow(&args.flow.flow_id, args.force, json_output)
                .await
        }
    }
}

async fn dispatch_job_command(
    client: &mut client::ShirohaClient,
    command: JobCommands,
    json_output: bool,
) -> anyhow::Result<()> {
    match command {
        JobCommands::New(args) => {
            client
                .create_job(
                    &args.flow.flow_id,
                    client::decode_optional_bytes(
                        args.context.context_text.as_deref(),
                        args.context.context_hex.as_deref(),
                        args.context.context_file.as_deref(),
                    )?,
                    args.max_lifetime_ms,
                    json_output,
                )
                .await
        }
        JobCommands::Get(args) => client.get_job(&args.job_id, json_output).await,
        JobCommands::Rm(args) => {
            client
                .delete_job(&args.job.job_id, args.force, json_output)
                .await
        }
        JobCommands::Ls(args) => {
            client
                .list_jobs(args.flow_id.as_deref(), args.all, json_output)
                .await
        }
        JobCommands::Trig(args) => {
            client
                .trigger_event(
                    &args.job.job_id,
                    &args.event,
                    client::decode_optional_bytes(
                        args.payload.payload_text.as_deref(),
                        args.payload.payload_hex.as_deref(),
                        args.payload.payload_file.as_deref(),
                    )?,
                    json_output,
                )
                .await
        }
        JobCommands::Pause(args) => client.pause_job(&args.job_id, json_output).await,
        JobCommands::Resume(args) => client.resume_job(&args.job_id, json_output).await,
        JobCommands::Cancel(args) => client.cancel_job(&args.job_id, json_output).await,
        JobCommands::Logs(args) => {
            client
                .get_job_events(
                    &args.job.job_id,
                    client::EventQueryOptions {
                        pretty: args.pretty,
                        follow: args.follow,
                        since_id: args.since_id,
                        since_timestamp_ms: args.since_timestamp_ms,
                        limit: args.limit,
                        kind_filters: args.kind,
                        tail: args.tail,
                        interval_ms: args.interval_ms,
                        json_output,
                    },
                )
                .await
        }
        JobCommands::Wait(args) => {
            client
                .wait_job(
                    &args.job.job_id,
                    args.state.as_deref(),
                    args.timeout_ms,
                    args.interval_ms,
                    json_output,
                )
                .await
        }
    }
}
