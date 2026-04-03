use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::Value;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn shirohad_binary() -> PathBuf {
    let root = workspace_root();
    let binary = root
        .join("target")
        .join("debug")
        .join(format!("shirohad{}", std::env::consts::EXE_SUFFIX));
    if binary.exists() {
        return binary;
    }

    let status = Command::new("cargo")
        .arg("build")
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

fn build_example(manifest_path: &str) -> PathBuf {
    let root = workspace_root();
    let manifest = root.join(manifest_path);
    let status = Command::new("cargo")
        .arg("build")
        .arg("--offline")
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
        .join("target/wasm32-wasip2/release/simple.wasm")
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
            if let Ok(output) = run_sctl(&self.server_addr, &["--json", "flows"])
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
    let example_wasm = build_example("example/simple/Cargo.toml");
    assert!(
        Path::new(&example_wasm).exists(),
        "simple example wasm should exist"
    );

    let flows = run_sctl(&server.server_addr, &["--json", "flows"]).expect("flows command");
    expect_success(&flows);
    assert_eq!(parse_json(&flows.stdout), Value::Array(Vec::new()));

    let jobs = run_sctl(
        &server.server_addr,
        &["--json", "jobs", "--flow-id", "simple"],
    )
    .expect("jobs command");
    expect_success(&jobs);
    assert_eq!(parse_json(&jobs.stdout), Value::Array(Vec::new()));

    let deploy = run_sctl(
        &server.server_addr,
        &[
            "--json",
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

    let flow = run_sctl(
        &server.server_addr,
        &["--json", "flow", "--flow-id", "simple"],
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
            "create",
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
            "trigger",
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
            "wait",
            "--job-id",
            &job_id,
            "--state",
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

    let events = run_sctl(
        &server.server_addr,
        &["--json", "events", "--job-id", &job_id],
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
}
