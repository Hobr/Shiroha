//! 测试辅助工具。
//!
//! 这里集中放 server harness、临时数据目录、WASM fixture 构造和
//! UDS gRPC 连接逻辑，避免各测试文件重复搭基础设施。

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::{Mutex as StdMutex, OnceLock};
use std::{fs, process::Command};

use hyper_util::rt::TokioIo;
use shiroha_core::flow::{
    ActionDef, DispatchMode, FlowManifest, StateDef, StateKind, TimeoutDef, TransitionDef,
};
use shiroha_proto::shiroha_api::ListFlowsRequest;
use shiroha_proto::shiroha_api::flow_service_client::FlowServiceClient;
use shiroha_proto::shiroha_api::job_service_client::JobServiceClient;
use tokio::net::UnixStream;
use tokio::sync::oneshot;
use tokio::time::{Duration, sleep};
use tonic::transport::{Channel, Endpoint};
use tower::service_fn;
use uuid::Uuid;

use crate::server::{ShirohaServer, ShirohaState, spawn_timer_forwarder};

pub(crate) struct TestHarness {
    pub(crate) state: Arc<ShirohaState>,
    pub(crate) data_dir: PathBuf,
    /// 只有需要真实 timer -> job 转发链路的测试才会启动这个任务。
    timer_forwarder: Option<tokio::task::JoinHandle<()>>,
}

/// 启动真实 gRPC server 的集成测试夹具。
///
/// 与 `TestHarness` 不同，它通过 Unix Domain Socket 暴露网络接口，
/// 适合覆盖 tonic client/server 往返链路。
pub(crate) struct LiveGrpcServer {
    pub(crate) data_dir: PathBuf,
    socket_path: PathBuf,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join_handle: Option<tokio::task::JoinHandle<anyhow::Result<()>>>,
}

impl TestHarness {
    pub(crate) async fn new(prefix: &str) -> Self {
        let data_dir = temp_data_dir(prefix);
        let server = ShirohaServer::new(data_dir.to_str().expect("utf-8 path"))
            .await
            .expect("create test server");
        let (state, _) = server.into_test_parts();

        Self {
            state,
            data_dir,
            timer_forwarder: None,
        }
    }

    pub(crate) async fn with_timer_forwarder(prefix: &str) -> Self {
        // 某些测试会直接操作 `JobServiceImpl`，这时需要手动补上 timer forwarder
        // 才能让 TimerWheel 事件真正进入 enqueue 路径。
        let data_dir = temp_data_dir(prefix);
        let server = ShirohaServer::new(data_dir.to_str().expect("utf-8 path"))
            .await
            .expect("create test server");
        let (state, timer_rx) = server.into_test_parts();
        let timer_forwarder = Some(spawn_timer_forwarder(state.clone(), timer_rx));

        Self {
            state,
            data_dir,
            timer_forwarder,
        }
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        if let Some(handle) = self.timer_forwarder.take() {
            handle.abort();
        }
    }
}

impl LiveGrpcServer {
    pub(crate) async fn start(prefix: &str) -> Self {
        let data_dir = temp_data_dir(prefix);
        let server = ShirohaServer::new(data_dir.to_str().expect("utf-8 path"))
            .await
            .expect("create test server");
        let socket_path = data_dir.join("shirohad.sock");
        let _ = std::fs::remove_file(&socket_path);
        let listener =
            tokio::net::UnixListener::bind(&socket_path).expect("bind unix domain socket");
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        let join_handle = tokio::spawn(async move {
            server
                .start_with_unix_listener(listener, async move {
                    let _ = shutdown_rx.await;
                })
                .await
        });

        let live = Self {
            data_dir,
            socket_path,
            shutdown_tx: Some(shutdown_tx),
            join_handle: Some(join_handle),
        };
        live.wait_until_ready().await;
        live
    }

    pub(crate) async fn flow_client(&self) -> FlowServiceClient<Channel> {
        FlowServiceClient::new(connect_channel(&self.socket_path).await)
    }

    pub(crate) async fn job_client(&self) -> JobServiceClient<Channel> {
        JobServiceClient::new(connect_channel(&self.socket_path).await)
    }

    async fn wait_until_ready(&self) {
        for _ in 0..40 {
            if let Ok(channel) = try_connect_channel(&self.socket_path).await {
                let mut client = FlowServiceClient::new(channel);
                // 选择最轻量的 RPC 探针，确认 server 已经完成 accept + service 注册。
                match client.list_flows(ListFlowsRequest {}).await {
                    Ok(_) => return,
                    Err(status) if status.code() != tonic::Code::Unavailable => return,
                    Err(_) => {}
                }
            }
            sleep(Duration::from_millis(10)).await;
        }
        panic!(
            "gRPC server did not become ready at {}",
            self.socket_path.display()
        );
    }
}

impl Drop for LiveGrpcServer {
    fn drop(&mut self) {
        if let Some(shutdown_tx) = self.shutdown_tx.take() {
            let _ = shutdown_tx.send(());
        }
        if let Some(join_handle) = self.join_handle.take() {
            join_handle.abort();
        }
        let _ = std::fs::remove_file(&self.socket_path);
        let _ = std::fs::remove_dir_all(&self.data_dir);
    }
}

pub(crate) fn approval_manifest(flow_id: &str, guard: Option<&str>) -> FlowManifest {
    // 最小 happy-path flow：一次 approve 事件驱动一次转移和一次 action。
    FlowManifest {
        id: flow_id.to_string(),
        states: vec![
            StateDef {
                name: "idle".into(),
                kind: StateKind::Normal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
            StateDef {
                name: "done".into(),
                kind: StateKind::Terminal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
        ],
        transitions: vec![TransitionDef {
            from: "idle".into(),
            to: "done".into(),
            event: "approve".into(),
            guard: guard.map(str::to_string),
            action: Some("ship".into()),
            timeout: None,
        }],
        initial_state: "idle".into(),
        actions: vec![
            ActionDef {
                name: "ship".into(),
                dispatch: DispatchMode::Local,
            },
            ActionDef {
                name: "allow".into(),
                dispatch: DispatchMode::Local,
            },
            ActionDef {
                name: "deny".into(),
                dispatch: DispatchMode::Local,
            },
        ],
    }
}

pub(crate) fn timeout_manifest(flow_id: &str) -> FlowManifest {
    // 专门用于覆盖 timeout -> enqueue_event -> terminal transition 这条链路。
    FlowManifest {
        id: flow_id.to_string(),
        states: vec![
            StateDef {
                name: "waiting".into(),
                kind: StateKind::Normal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
            StateDef {
                name: "timed_out".into(),
                kind: StateKind::Terminal,
                on_enter: None,
                on_exit: None,
                subprocess: None,
            },
        ],
        transitions: vec![TransitionDef {
            from: "waiting".into(),
            to: "timed_out".into(),
            event: "expire".into(),
            guard: None,
            action: None,
            timeout: Some(TimeoutDef {
                duration_ms: 25,
                timeout_event: "expire".into(),
            }),
        }],
        initial_state: "waiting".into(),
        actions: Vec::new(),
    }
}

pub(crate) fn wasm_for_manifest(manifest: &FlowManifest) -> Vec<u8> {
    let manifest_json = serde_json::to_string(manifest).expect("serialize manifest");
    // 生成结果按 manifest 内容缓存，避免每个测试都重新编译同一个 component fixture。
    let build_key = compute_hash(manifest_json.as_bytes());
    let build_root = std::env::temp_dir()
        .join("shiroha-component-fixtures")
        .join(&build_key);
    let target_dir = build_root.join("target");
    let wasm_path = target_dir.join("wasm32-wasip2/release/flow_component_fixture.wasm");

    if wasm_path.exists() {
        return fs::read(&wasm_path).expect("read cached component fixture");
    }

    static BUILD_LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    // cargo build 会写同一个 target 目录；串行化能避免并发测试互相踩缓存。
    let _guard = BUILD_LOCK
        .get_or_init(|| StdMutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if wasm_path.exists() {
        return fs::read(&wasm_path).expect("read cached component fixture");
    }

    fs::create_dir_all(&build_root).expect("create fixture build dir");
    let fixture_manifest =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/flow-component/Cargo.toml");
    let status = Command::new("cargo")
        .arg("build")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(&fixture_manifest)
        .arg("--target")
        .arg("wasm32-wasip2")
        .arg("--release")
        .env("SHIROHA_MANIFEST_JSON", &manifest_json)
        .env("CARGO_TARGET_DIR", &target_dir)
        .status()
        .expect("run cargo build for component fixture");

    assert!(status.success(), "component fixture build failed");
    fs::read(&wasm_path).expect("read built component fixture")
}

pub(crate) fn example_wasm(manifest_path: &str, package_name: &str) -> Vec<u8> {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest_path = workspace_root.join(manifest_path);
    let build_root = std::env::temp_dir()
        .join("shiroha-example-builds")
        .join(package_name);
    let target_dir = build_root.join("target");
    let wasm_path = target_dir.join(format!("wasm32-wasip2/release/{package_name}.wasm"));

    if wasm_path.exists() {
        return fs::read(&wasm_path).expect("read cached example component");
    }

    static BUILD_LOCK: OnceLock<StdMutex<()>> = OnceLock::new();
    let _guard = BUILD_LOCK
        .get_or_init(|| StdMutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if wasm_path.exists() {
        return fs::read(&wasm_path).expect("read cached example component");
    }

    fs::create_dir_all(&build_root).expect("create example build dir");
    let status = Command::new("cargo")
        .arg("build")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .arg("--target")
        .arg("wasm32-wasip2")
        .arg("--release")
        .env("CARGO_TARGET_DIR", &target_dir)
        .status()
        .expect("run cargo build for example component");

    assert!(status.success(), "example component build failed");
    fs::read(&wasm_path).expect("read built example component")
}

fn temp_data_dir(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("shiroha-{prefix}-{}", Uuid::now_v7()))
}

fn compute_hash(bytes: &[u8]) -> String {
    let len = bytes.len();
    let head: Vec<u8> = bytes.iter().take(16).copied().collect();
    let tail: Vec<u8> = bytes.iter().rev().take(16).copied().collect();
    format!("{len:016x}-{}-{}", hex(&head), hex(&tail))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

async fn connect_channel(socket_path: &std::path::Path) -> Channel {
    try_connect_channel(socket_path)
        .await
        .expect("connect grpc channel")
}

async fn try_connect_channel(
    socket_path: &std::path::Path,
) -> Result<Channel, tonic::transport::Error> {
    let socket_path = socket_path.to_path_buf();
    // tonic 仍要求一个 HTTP endpoint，这里只借用其配置对象，真正连接走 UDS connector。
    Endpoint::try_from("http://[::]:50051")
        .expect("valid endpoint")
        .connect_with_connector(service_fn(move |_| {
            let socket_path = socket_path.clone();
            async move { UnixStream::connect(socket_path).await.map(TokioIo::new) }
        }))
        .await
}
