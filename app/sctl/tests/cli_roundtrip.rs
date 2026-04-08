use std::fs::{File, OpenOptions};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex as StdMutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde_json::Value;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

struct CargoBuildLockGuard {
    _thread_guard: std::sync::MutexGuard<'static, ()>,
    file: File,
}

impl Drop for CargoBuildLockGuard {
    fn drop(&mut self) {
        self.file
            .unlock()
            .expect("unlock cross-process cargo build lock");
    }
}

fn shirohad_binary() -> PathBuf {
    let root = workspace_root();
    let binary = root
        .join("target")
        .join("debug")
        .join(format!("shirohad{}", std::env::consts::EXE_SUFFIX));
    let _guard = acquire_cargo_build_lock();
    // 这里始终重建一次，避免 ignored round-trip 复用过期的 `target/debug/shirohad`
    // 导致新增 RPC 没有进入真实服务进程。
    let status = Command::new("cargo")
        .arg("build")
        .arg("--jobs")
        .arg("1")
        .arg("-p")
        .arg("shirohad")
        .current_dir(&root)
        .status()
        .expect("build shirohad");
    assert!(status.success(), "failed to build shirohad");
    binary
}

fn sctl_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sctl"))
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("shiroha-{prefix}-{nonce}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn build_example(manifest_path: &str, package_name: &str) -> PathBuf {
    let root = workspace_root();
    let manifest = root.join(manifest_path);
    let _guard = acquire_cargo_build_lock();
    let status = Command::new("cargo")
        .arg("build")
        .arg("--offline")
        .arg("--jobs")
        .arg("1")
        .arg("--manifest-path")
        .arg(&manifest)
        .arg("--target")
        .arg("wasm32-wasip2")
        .arg("--release")
        .current_dir(&root)
        .status()
        .expect("build example");
    assert!(status.success(), "example build failed");

    manifest
        .parent()
        .expect("example dir")
        .join(format!("target/wasm32-wasip2/release/{package_name}.wasm"))
}

struct RunningServer {
    child: Child,
    server_addr: String,
    data_dir: PathBuf,
}

impl RunningServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
        let port = listener.local_addr().expect("local addr").port();
        drop(listener);

        let server_addr = format!("http://127.0.0.1:{port}");
        let data_dir = unique_temp_dir("sctl-cli");
        let child = Command::new(shirohad_binary())
            .arg("--listen")
            .arg(server_addr.trim_start_matches("http://"))
            .arg("--data-dir")
            .arg(&data_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn shirohad");

        let server = Self {
            child,
            server_addr,
            data_dir,
        };
        server.wait_until_ready();
        server
    }

    fn wait_until_ready(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Ok(output) = run_sctl(&self.server_addr, &["--json", "flow", "ls"])
                && output.status.success()
            {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if serde_json::from_str::<Value>(&stdout).is_ok() {
                    return;
                }
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("shirohad did not become ready at {}", self.server_addr);
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}

fn run_sctl(server_addr: &str, args: &[&str]) -> std::io::Result<std::process::Output> {
    let mut command = Command::new(sctl_binary());
    command.arg("--server").arg(server_addr);
    command.args(args);
    command.output()
}

fn parse_json(stdout: &[u8]) -> Value {
    serde_json::from_slice(stdout).expect("stdout should be valid json")
}

fn temp_file_path(prefix: &str, extension: &str) -> PathBuf {
    unique_temp_dir(prefix).join(format!("sctl-complete.{extension}"))
}

fn acquire_cargo_build_lock() -> CargoBuildLockGuard {
    static BUILD_LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    let thread_guard = BUILD_LOCK
        .get_or_init(|| StdMutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let lock_dir = std::env::temp_dir().join("shiroha-build-locks");
    std::fs::create_dir_all(&lock_dir).expect("create cargo build lock dir");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_dir.join("cargo-build.lock"))
        .expect("open cross-process cargo build lock");
    file.lock_exclusive()
        .expect("acquire cross-process cargo build lock");

    CargoBuildLockGuard {
        _thread_guard: thread_guard,
        file,
    }
}

#[test]
fn complete_command_emits_bash_script() {
    let output = Command::new(sctl_binary())
        .args(["complete", "bash"])
        .output()
        .expect("complete bash command");
    expect_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("_clap_complete_sctl"));
    assert!(stdout.contains("COMPLETE=\"bash\""));
}

#[test]
fn complete_command_emits_fish_script() {
    let output = Command::new(sctl_binary())
        .args(["complete", "fish"])
        .output()
        .expect("complete fish command");
    expect_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("complete --keep-order --exclusive --command sctl"));
    assert!(stdout.contains("COMPLETE=fish"));
}

#[test]
fn flow_help_mentions_summary_flag() {
    let output = Command::new(sctl_binary())
        .args(["flow", "get", "--help"])
        .output()
        .expect("flow get help command");
    expect_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--summary"));
    assert!(stdout.contains("--version"));
}

#[test]
fn delete_flow_help_mentions_flow_id() {
    let output = Command::new(sctl_binary())
        .args(["flow", "rm", "--help"])
        .output()
        .expect("flow delete help command");
    expect_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--flow-id"));
    assert!(stdout.contains("--force"));
}

#[test]
fn flow_versions_help_mentions_flow_id() {
    let output = Command::new(sctl_binary())
        .args(["flow", "vers", "--help"])
        .output()
        .expect("flow versions help command");
    expect_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--flow-id"));
}

#[test]
fn jobs_help_mentions_all_flag() {
    let output = Command::new(sctl_binary())
        .args(["job", "ls", "--help"])
        .output()
        .expect("job list help command");
    expect_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--all"));
    assert!(stdout.contains("--flow-id"));
}

#[test]
fn events_help_mentions_kind_and_tail_flags() {
    let output = Command::new(sctl_binary())
        .args(["job", "logs", "--help"])
        .output()
        .expect("job events help command");
    expect_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--kind"));
    assert!(stdout.contains("--tail"));
    assert!(stdout.contains("--since-id"));
    assert!(stdout.contains("--since-timestamp-ms"));
    assert!(stdout.contains("--limit"));
}

#[test]
fn delete_job_help_mentions_job_id() {
    let output = Command::new(sctl_binary())
        .args(["job", "rm", "--help"])
        .output()
        .expect("job delete help command");
    expect_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--job-id"));
    assert!(stdout.contains("--force"));
}

#[test]
fn complete_command_prints_default_fish_path() {
    let output = Command::new(sctl_binary())
        .env("HOME", "/tmp/sctl-home")
        .args(["complete", "fish", "--print-path"])
        .output()
        .expect("complete fish print-path command");
    expect_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        "/tmp/sctl-home/.config/fish/completions/sctl.fish"
    );
}

#[test]
fn complete_command_writes_script_to_explicit_output() {
    let output_path = temp_file_path("sctl-complete-output", "fish");
    let output_dir = output_path.parent().expect("temp output dir").to_path_buf();
    let output = Command::new(sctl_binary())
        .args([
            "complete",
            "fish",
            "--output",
            output_path.to_str().expect("utf-8 temp path"),
        ])
        .output()
        .expect("complete fish output command");
    expect_success(&output);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.trim(),
        output_path.to_str().expect("utf-8 temp path")
    );

    let script = std::fs::read_to_string(&output_path).expect("read written fish completion");
    assert!(script.contains("complete --keep-order --exclusive --command sctl"));
    assert!(script.contains("COMPLETE=fish"));

    let _ = std::fs::remove_dir_all(&output_dir);
}

fn expect_success(output: &std::process::Output) {
    assert!(
        output.status.success(),
        "command failed: status={:?}\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "requires spawning shirohad on a local TCP port"]
fn cli_json_round_trip_against_real_server() {
    let server = RunningServer::start();
    let example_wasm = build_example("example/simple/Cargo.toml", "simple");
    assert!(
        Path::new(&example_wasm).exists(),
        "simple example wasm should exist"
    );

    let flows = run_sctl(&server.server_addr, &["--json", "flow", "ls"]).expect("flows command");
    expect_success(&flows);
    assert_eq!(parse_json(&flows.stdout), Value::Array(Vec::new()));

    let jobs = run_sctl(
        &server.server_addr,
        &["--json", "job", "ls", "--flow-id", "simple"],
    )
    .expect("jobs command");
    expect_success(&jobs);
    assert_eq!(parse_json(&jobs.stdout), Value::Array(Vec::new()));

    let deploy = run_sctl(
        &server.server_addr,
        &[
            "--json",
            "flow",
            "deploy",
            "--file",
            example_wasm.to_str().expect("utf-8 path"),
            "--flow-id",
            "simple",
        ],
    )
    .expect("deploy command");
    expect_success(&deploy);
    let deploy_json = parse_json(&deploy.stdout);
    assert_eq!(deploy_json["flow_id"], "simple");
    assert!(deploy_json["manifest"].is_object());
    assert_eq!(deploy_json["warnings"], Value::Array(Vec::new()));

    let flow = run_sctl(
        &server.server_addr,
        &["--json", "flow", "get", "--flow-id", "simple"],
    )
    .expect("flow command");
    expect_success(&flow);
    let flow_json = parse_json(&flow.stdout);
    assert_eq!(flow_json["flow_id"], "simple");
    assert_eq!(flow_json["manifest"]["initial_state"], "pending-approval");

    let create = run_sctl(
        &server.server_addr,
        &[
            "--json",
            "job",
            "new",
            "--flow-id",
            "simple",
            "--context-text",
            "demo-request",
        ],
    )
    .expect("create command");
    expect_success(&create);
    let create_json = parse_json(&create.stdout);
    let job_id = create_json["job_id"]
        .as_str()
        .expect("job_id string")
        .to_string();

    let trigger = run_sctl(
        &server.server_addr,
        &[
            "--json",
            "job",
            "trig",
            "--job-id",
            &job_id,
            "--event",
            "approve",
            "--payload-text",
            "approved-by-cli",
        ],
    )
    .expect("trigger command");
    expect_success(&trigger);
    let trigger_json = parse_json(&trigger.stdout);
    assert_eq!(trigger_json["event"], "approve");

    let wait = run_sctl(
        &server.server_addr,
        &[
            "--json",
            "job",
            "wait",
            "--job-id",
            &job_id,
            "--current-state",
            "approved",
            "--timeout-ms",
            "5000",
            "--interval-ms",
            "100",
        ],
    )
    .expect("wait command");
    expect_success(&wait);
    let wait_json = parse_json(&wait.stdout);
    assert_eq!(wait_json["state"], "completed");
    assert_eq!(wait_json["current_state"], "approved");
    assert!(wait_json["flow_version"].is_string());
    assert_eq!(wait_json["context_bytes"], 12);

    let jobs_all =
        run_sctl(&server.server_addr, &["--json", "job", "ls", "--all"]).expect("jobs all command");
    expect_success(&jobs_all);
    let jobs_all_json = parse_json(&jobs_all.stdout);
    let jobs_all = jobs_all_json.as_array().expect("jobs array");
    assert_eq!(jobs_all.len(), 1);
    assert_eq!(jobs_all[0]["job_id"].as_str(), Some(job_id.as_str()));
    assert!(jobs_all[0]["flow_version"].is_string());
    assert_eq!(jobs_all[0]["context_bytes"], 12);

    let flow_versions = run_sctl(
        &server.server_addr,
        &["--json", "flow", "vers", "--flow-id", "simple"],
    )
    .expect("flow versions command");
    expect_success(&flow_versions);
    let flow_versions_json = parse_json(&flow_versions.stdout);
    let flow_versions = flow_versions_json.as_array().expect("flow versions array");
    assert_eq!(flow_versions.len(), 1);
    let deployed_version = deploy_json["version"]
        .as_str()
        .expect("deployed version string");
    assert_eq!(flow_versions[0]["version"], deployed_version);

    let flow_by_version = run_sctl(
        &server.server_addr,
        &[
            "--json",
            "flow",
            "get",
            "--flow-id",
            "simple",
            "--version",
            deployed_version,
        ],
    )
    .expect("flow by version command");
    expect_success(&flow_by_version);
    let flow_by_version_json = parse_json(&flow_by_version.stdout);
    assert_eq!(flow_by_version_json["version"], deployed_version);

    let events = run_sctl(
        &server.server_addr,
        &["--json", "job", "logs", "--job-id", &job_id],
    )
    .expect("events command");
    expect_success(&events);
    let events_json = parse_json(&events.stdout);
    let events = events_json.as_array().expect("events array");
    assert_eq!(events.len(), 4);
    assert_eq!(events[0]["kind"]["type"], "created");
    assert_eq!(events[1]["kind"]["type"], "transition");
    assert_eq!(events[2]["kind"]["type"], "action_complete");
    assert_eq!(events[3]["kind"]["type"], "completed");

    let filtered_events = run_sctl(
        &server.server_addr,
        &[
            "--json",
            "job",
            "logs",
            "--job-id",
            &job_id,
            "--kind",
            "transition",
            "--tail",
            "1",
        ],
    )
    .expect("filtered events command");
    expect_success(&filtered_events);
    let filtered_events_json = parse_json(&filtered_events.stdout);
    let filtered_events = filtered_events_json
        .as_array()
        .expect("filtered events array");
    assert_eq!(filtered_events.len(), 1);
    assert_eq!(filtered_events[0]["kind"]["type"], "transition");

    let since_id = events[0]["id"].as_str().expect("first event id");
    let incremental_events = run_sctl(
        &server.server_addr,
        &[
            "--json",
            "job",
            "logs",
            "--job-id",
            &job_id,
            "--since-id",
            since_id,
            "--kind",
            "transition",
            "--limit",
            "1",
        ],
    )
    .expect("incremental events command");
    expect_success(&incremental_events);
    let incremental_events_json = parse_json(&incremental_events.stdout);
    let incremental_events = incremental_events_json
        .as_array()
        .expect("incremental events array");
    assert_eq!(incremental_events.len(), 1);
    assert_eq!(incremental_events[0]["kind"]["type"], "transition");

    let delete_job = run_sctl(
        &server.server_addr,
        &["--json", "job", "rm", "--job-id", &job_id],
    )
    .expect("delete job command");
    expect_success(&delete_job);
    let delete_job_json = parse_json(&delete_job.stdout);
    assert_eq!(delete_job_json["job_id"], job_id);

    let jobs_after_delete =
        run_sctl(&server.server_addr, &["--json", "job", "ls", "--all"]).expect("jobs all command");
    expect_success(&jobs_after_delete);
    assert_eq!(
        parse_json(&jobs_after_delete.stdout),
        Value::Array(Vec::new())
    );

    let delete_flow = run_sctl(
        &server.server_addr,
        &["--json", "flow", "rm", "--flow-id", "simple"],
    )
    .expect("delete flow command");
    expect_success(&delete_flow);
    let delete_flow_json = parse_json(&delete_flow.stdout);
    assert_eq!(delete_flow_json["flow_id"], "simple");

    let flows_after_delete =
        run_sctl(&server.server_addr, &["--json", "flow", "ls"]).expect("flows command");
    expect_success(&flows_after_delete);
    assert_eq!(
        parse_json(&flows_after_delete.stdout),
        Value::Array(Vec::new())
    );
}

#[test]
#[ignore = "requires spawning shirohad on a local TCP port"]
fn deploy_warning_example_reports_warnings_in_json() {
    let server = RunningServer::start();
    let example_wasm = build_example("example/warning-deadlock/Cargo.toml", "warning_deadlock");
    assert!(
        Path::new(&example_wasm).exists(),
        "warning example wasm should exist"
    );

    let deploy = run_sctl(
        &server.server_addr,
        &[
            "--json",
            "flow",
            "deploy",
            "--file",
            example_wasm.to_str().expect("utf-8 path"),
            "--flow-id",
            "warning-deadlock",
        ],
    )
    .expect("deploy warning example");
    expect_success(&deploy);

    let deploy_json = parse_json(&deploy.stdout);
    let warnings = deploy_json["warnings"]
        .as_array()
        .expect("warnings array should exist");
    assert!(!warnings.is_empty(), "expected validator warnings");
    assert!(warnings.iter().any(|warning| {
        warning
            .as_str()
            .is_some_and(|warning| warning.contains("cannot reach any terminal state"))
    }));
}

#[test]
#[ignore = "requires spawning shirohad on a local TCP port"]
fn force_delete_can_remove_running_job_and_flow_with_jobs() {
    let server = RunningServer::start();
    let example_wasm = build_example("example/simple/Cargo.toml", "simple");

    let deploy = run_sctl(
        &server.server_addr,
        &[
            "--json",
            "flow",
            "deploy",
            "--file",
            example_wasm.to_str().expect("utf-8 path"),
            "--flow-id",
            "simple",
        ],
    )
    .expect("deploy simple flow");
    expect_success(&deploy);

    let create = run_sctl(
        &server.server_addr,
        &[
            "--json",
            "job",
            "new",
            "--flow-id",
            "simple",
            "--context-text",
            "running-job",
        ],
    )
    .expect("create running job");
    expect_success(&create);
    let create_json = parse_json(&create.stdout);
    let job_id = create_json["job_id"]
        .as_str()
        .expect("job_id string")
        .to_string();

    let force_job_delete = run_sctl(
        &server.server_addr,
        &["--json", "job", "rm", "--job-id", &job_id, "--force"],
    )
    .expect("force delete job");
    expect_success(&force_job_delete);
    let force_job_delete_json = parse_json(&force_job_delete.stdout);
    assert_eq!(force_job_delete_json["job_id"], job_id);
    assert_eq!(force_job_delete_json["forced"], true);
    assert_eq!(force_job_delete_json["previous_state"], "running");
    assert_eq!(force_job_delete_json["cancelled_before_delete"], true);

    let jobs_after_force_job =
        run_sctl(&server.server_addr, &["--json", "job", "ls", "--all"]).expect("list jobs");
    expect_success(&jobs_after_force_job);
    assert_eq!(
        parse_json(&jobs_after_force_job.stdout),
        Value::Array(Vec::new())
    );

    let create_second = run_sctl(
        &server.server_addr,
        &[
            "--json",
            "job",
            "new",
            "--flow-id",
            "simple",
            "--context-text",
            "job-for-flow-force",
        ],
    )
    .expect("create second running job");
    expect_success(&create_second);

    let force_flow_delete = run_sctl(
        &server.server_addr,
        &["--json", "flow", "rm", "--flow-id", "simple", "--force"],
    )
    .expect("force delete flow");
    expect_success(&force_flow_delete);
    let force_flow_delete_json = parse_json(&force_flow_delete.stdout);
    assert_eq!(force_flow_delete_json["flow_id"], "simple");
    assert_eq!(force_flow_delete_json["forced"], true);
    let deleted_jobs = force_flow_delete_json["deleted_jobs"]
        .as_array()
        .expect("deleted_jobs array");
    assert_eq!(deleted_jobs.len(), 1);
    assert_eq!(deleted_jobs[0]["previous_state"], "running");
    assert_eq!(deleted_jobs[0]["cancelled_before_delete"], true);

    let flows_after_force_flow =
        run_sctl(&server.server_addr, &["--json", "flow", "ls"]).expect("list flows");
    expect_success(&flows_after_force_flow);
    assert_eq!(
        parse_json(&flows_after_force_flow.stdout),
        Value::Array(Vec::new())
    );
}
