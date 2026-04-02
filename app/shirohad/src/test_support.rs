use std::path::PathBuf;
use std::sync::Arc;

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

const ACTION_RESULT_JSON: &str = r#"{"status":"success","output":[79,75]}"#;
const AGGREGATE_RESULT_JSON: &str = r#"{"event":"done"}"#;

pub(crate) struct TestHarness {
    pub(crate) state: Arc<ShirohaState>,
    pub(crate) data_dir: PathBuf,
    timer_forwarder: Option<tokio::task::JoinHandle<()>>,
}

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
    let manifest_data = wat_bytes_literal(manifest_json.as_bytes());
    let action_data = wat_bytes_literal(ACTION_RESULT_JSON.as_bytes());
    let aggregate_data = wat_bytes_literal(AGGREGATE_RESULT_JSON.as_bytes());
    let manifest_len = manifest_json.len();
    let action_len = ACTION_RESULT_JSON.len();
    let aggregate_len = AGGREGATE_RESULT_JSON.len();

    format!(
        r#"(module
          (memory (export "memory") 1)
          (global $heap (mut i32) (i32.const 4096))

          (data (i32.const 0) "{manifest_data}")
          (data (i32.const 2048) "{action_data}")
          (data (i32.const 3072) "{aggregate_data}")

          (func (export "alloc") (param $len i32) (result i32)
            (local $ptr i32)
            global.get $heap
            local.tee $ptr
            local.get $len
            i32.add
            global.set $heap
            local.get $ptr)

          (func $pack (param $ptr i32) (param $len i32) (result i64)
            local.get $ptr
            i64.extend_i32_u
            i64.const 32
            i64.shl
            local.get $len
            i64.extend_i32_u
            i64.or)

          (func (export "get-manifest") (result i64)
            i32.const 0
            i32.const {manifest_len}
            call $pack)

          (func (export "invoke-action") (param $name_ptr i32) (param $name_len i32) (param $ctx_ptr i32) (param $ctx_len i32) (result i64)
            local.get $name_ptr
            i32.load8_u
            drop
            local.get $ctx_ptr
            i32.load8_u
            drop
            i32.const 2048
            i32.const {action_len}
            call $pack)

          (func (export "invoke-guard") (param $name_ptr i32) (param $name_len i32) (param $ctx_ptr i32) (param $ctx_len i32) (result i32)
            local.get $ctx_ptr
            i32.load8_u
            drop
            local.get $name_ptr
            i32.load8_u
            i32.const 97
            i32.eq)

          (func (export "aggregate") (param $name_ptr i32) (param $name_len i32) (param $results_ptr i32) (param $results_len i32) (result i64)
            local.get $name_ptr
            i32.load8_u
            drop
            local.get $results_ptr
            i32.load8_u
            drop
            i32.const 3072
            i32.const {aggregate_len}
            call $pack))"#
    )
    .into_bytes()
}

fn wat_bytes_literal(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!(r"\{byte:02x}")).collect()
}

fn temp_data_dir(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("shiroha-{prefix}-{}", Uuid::now_v7()))
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
    Endpoint::try_from("http://[::]:50051")
        .expect("valid endpoint")
        .connect_with_connector(service_fn(move |_| {
            let socket_path = socket_path.clone();
            async move { UnixStream::connect(socket_path).await.map(TokioIo::new) }
        }))
        .await
}
