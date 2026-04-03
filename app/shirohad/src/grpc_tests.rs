//! 端到端 gRPC 集成测试。
//!
//! 这些测试通过真实的 tonic client + UDS server 覆盖“协议层 + 服务层 + 引擎层”
//! 的完整往返路径，和直接调用 `JobServiceImpl` 的单元测试互补。

use tokio::time::{Duration, sleep, timeout};

use crate::test_support::{
    LiveGrpcServer, approval_manifest, example_wasm, timeout_manifest, wasm_for_manifest,
};
use shiroha_core::event::EventKind;
use shiroha_proto::shiroha_api::flow_service_client::FlowServiceClient;
use shiroha_proto::shiroha_api::job_service_client::JobServiceClient;
use shiroha_proto::shiroha_api::{
    CreateJobRequest, DeployFlowRequest, GetFlowRequest, GetJobEventsRequest, GetJobRequest,
    ListFlowsRequest, PauseJobRequest, ResumeJobRequest, TriggerEventRequest,
};
use tonic::transport::Channel;

async fn wait_for_job(
    client: &mut JobServiceClient<Channel>,
    job_id: &str,
    expected_state: &str,
    expected_current_state: &str,
) {
    // 轮询而不是订阅流式事件，保持测试夹具简单，同时仍能覆盖异步状态推进。
    timeout(Duration::from_millis(500), async {
        loop {
            let job = client
                .get_job(GetJobRequest {
                    job_id: job_id.to_string(),
                })
                .await
                .expect("get job")
                .into_inner();
            if job.state == expected_state && job.current_state == expected_current_state {
                return;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("job should reach expected state");
}

async fn deploy_manifest(
    client: &mut FlowServiceClient<Channel>,
    flow_id: &str,
    manifest: &shiroha_core::flow::FlowManifest,
) {
    // 测试和生产一样走 deploy_flow RPC，而不是直接写 storage，
    // 这样可以覆盖 component 编译、manifest 提取和内存缓存注册。
    client
        .deploy_flow(DeployFlowRequest {
            flow_id: flow_id.to_string(),
            wasm_bytes: wasm_for_manifest(manifest),
        })
        .await
        .expect("deploy flow");
}

async fn deploy_wasm(client: &mut FlowServiceClient<Channel>, flow_id: &str, wasm_bytes: Vec<u8>) {
    client
        .deploy_flow(DeployFlowRequest {
            flow_id: flow_id.to_string(),
            wasm_bytes,
        })
        .await
        .expect("deploy flow");
}

#[tokio::test]
async fn grpc_round_trip_flow_and_job_execution() {
    // 覆盖最完整的 happy-path：deploy -> create -> trigger -> complete -> event log。
    let server = LiveGrpcServer::start("grpc-roundtrip").await;
    let mut flow = server.flow_client().await;
    let mut job = server.job_client().await;
    let manifest = approval_manifest("grpc-approval", Some("allow"));

    deploy_manifest(&mut flow, "grpc-approval", &manifest).await;

    let listed = flow
        .list_flows(ListFlowsRequest {})
        .await
        .expect("list flows")
        .into_inner();
    assert_eq!(listed.flows.len(), 1);
    assert_eq!(listed.flows[0].flow_id, "grpc-approval");

    let fetched = flow
        .get_flow(GetFlowRequest {
            flow_id: "grpc-approval".into(),
        })
        .await
        .expect("get flow")
        .into_inner();
    assert_eq!(fetched.flow_id, "grpc-approval");

    let created = job
        .create_job(CreateJobRequest {
            flow_id: "grpc-approval".into(),
            context: Some(vec![1, 2, 3]),
        })
        .await
        .expect("create job")
        .into_inner();

    job.trigger_event(TriggerEventRequest {
        job_id: created.job_id.clone(),
        event: "approve".into(),
        payload: Some(vec![9]),
    })
    .await
    .expect("trigger event");

    wait_for_job(&mut job, &created.job_id, "completed", "done").await;

    let events = job
        .get_job_events(GetJobEventsRequest {
            job_id: created.job_id,
        })
        .await
        .expect("get events")
        .into_inner();
    let kinds: Vec<EventKind> = events
        .events
        .into_iter()
        .map(|event| serde_json::from_str(&event.kind_json).expect("event json"))
        .collect();

    assert_eq!(kinds.len(), 4);
    assert!(matches!(kinds[0], EventKind::Created { .. }));
    assert!(matches!(kinds[1], EventKind::Transition { .. }));
    assert!(matches!(kinds[2], EventKind::ActionComplete { .. }));
    assert!(matches!(kinds[3], EventKind::Completed { .. }));
}

#[tokio::test]
async fn grpc_pause_resume_processes_queued_event() {
    // 验证 pause 后的 trigger-event 会先排队，resume 时再继续消费。
    let server = LiveGrpcServer::start("grpc-pause-resume").await;
    let mut flow = server.flow_client().await;
    let mut job = server.job_client().await;

    deploy_manifest(
        &mut flow,
        "grpc-approval",
        &approval_manifest("grpc-approval", Some("allow")),
    )
    .await;

    let created = job
        .create_job(CreateJobRequest {
            flow_id: "grpc-approval".into(),
            context: None,
        })
        .await
        .expect("create job")
        .into_inner();

    job.pause_job(PauseJobRequest {
        job_id: created.job_id.clone(),
    })
    .await
    .expect("pause job");

    job.trigger_event(TriggerEventRequest {
        job_id: created.job_id.clone(),
        event: "approve".into(),
        payload: None,
    })
    .await
    .expect("trigger queued event");

    let paused = job
        .get_job(GetJobRequest {
            job_id: created.job_id.clone(),
        })
        .await
        .expect("get paused job")
        .into_inner();
    assert_eq!(paused.state, "paused");
    assert_eq!(paused.current_state, "idle");

    job.resume_job(ResumeJobRequest {
        job_id: created.job_id.clone(),
    })
    .await
    .expect("resume job");

    wait_for_job(&mut job, &created.job_id, "completed", "done").await;
}

#[tokio::test]
async fn grpc_timer_event_completes_job() {
    // 覆盖定时器通过 server 内部 forwarder 回注到 JobService 的路径。
    let server = LiveGrpcServer::start("grpc-timer").await;
    let mut flow = server.flow_client().await;
    let mut job = server.job_client().await;

    deploy_manifest(&mut flow, "grpc-timer", &timeout_manifest("grpc-timer")).await;

    let created = job
        .create_job(CreateJobRequest {
            flow_id: "grpc-timer".into(),
            context: None,
        })
        .await
        .expect("create job")
        .into_inner();

    wait_for_job(&mut job, &created.job_id, "completed", "timed_out").await;

    let events = job
        .get_job_events(GetJobEventsRequest {
            job_id: created.job_id,
        })
        .await
        .expect("get events")
        .into_inner();
    let kinds: Vec<EventKind> = events
        .events
        .into_iter()
        .map(|event| serde_json::from_str(&event.kind_json).expect("event json"))
        .collect();

    assert!(kinds.iter().any(
        |kind| matches!(kind, EventKind::Completed { final_state } if final_state == "timed_out")
    ));
}

#[tokio::test]
async fn grpc_simple_example_component_runs_end_to_end() {
    let server = LiveGrpcServer::start("grpc-example-simple").await;
    let mut flow = server.flow_client().await;
    let mut job = server.job_client().await;

    deploy_wasm(
        &mut flow,
        "simple",
        example_wasm("example/simple/Cargo.toml", "simple"),
    )
    .await;

    let created = job
        .create_job(CreateJobRequest {
            flow_id: "simple".into(),
            context: Some(b"demo-request".to_vec()),
        })
        .await
        .expect("create job")
        .into_inner();

    job.trigger_event(TriggerEventRequest {
        job_id: created.job_id.clone(),
        event: "approve".into(),
        payload: Some(b"approved-by-test".to_vec()),
    })
    .await
    .expect("trigger approve");

    wait_for_job(&mut job, &created.job_id, "completed", "approved").await;
}

#[tokio::test]
async fn grpc_advanced_example_runs_supported_submit_path() {
    let server = LiveGrpcServer::start("grpc-example-advanced").await;
    let mut flow = server.flow_client().await;
    let mut job = server.job_client().await;

    deploy_wasm(
        &mut flow,
        "advanced",
        example_wasm("example/advanced/Cargo.toml", "advanced"),
    )
    .await;

    let created = job
        .create_job(CreateJobRequest {
            flow_id: "advanced".into(),
            context: Some(b"quote-request".to_vec()),
        })
        .await
        .expect("create job")
        .into_inner();

    job.trigger_event(TriggerEventRequest {
        job_id: created.job_id.clone(),
        event: "submit".into(),
        payload: Some(b"draft-ready".to_vec()),
    })
    .await
    .expect("trigger submit");

    wait_for_job(&mut job, &created.job_id, "running", "legal-review").await;

    let events = job
        .get_job_events(GetJobEventsRequest {
            job_id: created.job_id,
        })
        .await
        .expect("get events")
        .into_inner();
    let kinds: Vec<EventKind> = events
        .events
        .into_iter()
        .map(|event| serde_json::from_str(&event.kind_json).expect("event json"))
        .collect();

    assert!(matches!(kinds[0], EventKind::Created { .. }));
    assert!(matches!(kinds[1], EventKind::Transition { .. }));
    assert!(matches!(
        &kinds[2],
        EventKind::ActionComplete { action, .. } if action == "normalize-request"
    ));
}

#[tokio::test]
async fn grpc_subprocess_examples_support_manual_parent_child_progression() {
    let server = LiveGrpcServer::start("grpc-example-sub").await;
    let mut flow = server.flow_client().await;
    let mut job = server.job_client().await;

    deploy_wasm(
        &mut flow,
        "legal-review-demo",
        example_wasm("example/sub/child/Cargo.toml", "child"),
    )
    .await;
    deploy_wasm(
        &mut flow,
        "purchase-parent-demo",
        example_wasm("example/sub/parent/Cargo.toml", "parent"),
    )
    .await;

    let child_job = job
        .create_job(CreateJobRequest {
            flow_id: "legal-review-demo".into(),
            context: None,
        })
        .await
        .expect("create child job")
        .into_inner();
    job.trigger_event(TriggerEventRequest {
        job_id: child_job.job_id.clone(),
        event: "approve".into(),
        payload: None,
    })
    .await
    .expect("trigger child approve");
    wait_for_job(&mut job, &child_job.job_id, "completed", "approved").await;

    let parent_job = job
        .create_job(CreateJobRequest {
            flow_id: "purchase-parent-demo".into(),
            context: None,
        })
        .await
        .expect("create parent job")
        .into_inner();
    job.trigger_event(TriggerEventRequest {
        job_id: parent_job.job_id.clone(),
        event: "submit".into(),
        payload: Some(b"legal-review-request".to_vec()),
    })
    .await
    .expect("trigger submit");
    wait_for_job(&mut job, &parent_job.job_id, "running", "legal-review").await;

    job.trigger_event(TriggerEventRequest {
        job_id: parent_job.job_id.clone(),
        event: "legal-review-complete".into(),
        payload: None,
    })
    .await
    .expect("simulate child completion");
    wait_for_job(&mut job, &parent_job.job_id, "completed", "approved").await;
}
