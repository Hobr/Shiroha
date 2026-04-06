//! 测试用 WASM component fixture。
//!
//! 该 crate 通过环境变量注入 manifest JSON，再导出固定的
//! `get_manifest / invoke_action / invoke_guard / aggregate` 实现，
//! 供宿主侧测试 host-guest 交互链路。

use serde::Deserialize;

wit_bindgen::generate!({
    path: "../../../../crate/shiroha-wasm/wit",
    world: "flow",
});

struct FlowComponent;

#[derive(Debug, Deserialize)]
struct ManifestTemplate {
    id: String,
    #[serde(default, alias = "host_world")]
    world: Option<FlowWorldTemplate>,
    states: Vec<StateTemplate>,
    transitions: Vec<TransitionTemplate>,
    initial_state: String,
    actions: Vec<ActionTemplate>,
}

#[derive(Debug, Deserialize)]
struct StateTemplate {
    name: String,
    kind: StateKindTemplate,
    on_enter: Option<String>,
    on_exit: Option<String>,
    subprocess: Option<SubprocessTemplate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StateKindTemplate {
    Normal,
    Terminal,
    Fork,
    Join,
    Subprocess,
}

#[derive(Debug, Deserialize)]
struct SubprocessTemplate {
    flow_id: String,
    completion_event: String,
}

#[derive(Debug, Deserialize)]
struct TransitionTemplate {
    from: String,
    to: String,
    event: String,
    guard: Option<String>,
    action: Option<String>,
    timeout: Option<TimeoutTemplate>,
}

#[derive(Debug, Deserialize)]
struct TimeoutTemplate {
    duration_ms: u64,
    timeout_event: String,
}

#[derive(Debug, Deserialize)]
struct ActionTemplate {
    name: String,
    dispatch: DispatchTemplate,
    #[serde(default)]
    capabilities: Vec<ActionCapabilityTemplate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ActionCapabilityTemplate {
    Network,
    Storage,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DispatchTemplate {
    Local,
    Remote,
    FanOut(FanOutTemplate),
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FlowWorldTemplate {
    Sandbox,
    Network,
    Storage,
    Full,
}

#[derive(Debug, Deserialize)]
struct FanOutTemplate {
    strategy: FanOutStrategyTemplate,
    aggregator: String,
    timeout_ms: Option<u64>,
    min_success: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum FanOutStrategyTemplate {
    All,
    Count(u32),
    Tagged(Vec<String>),
}

impl Guest for FlowComponent {
    fn get_manifest() -> FlowManifest {
        // 构建脚本把 manifest JSON 烘焙进环境变量，这样测试可以按需生成不同的 component。
        let manifest: ManifestTemplate = serde_json::from_str(env!("SHIROHA_MANIFEST_JSON"))
            .expect("fixture manifest json must be valid");
        manifest.into()
    }

    fn invoke_action(name: String, ctx: ActionContext) -> ActionResult {
        match name.as_str() {
            "ship" | "enter" | "exit" => {
                let payload_len = ctx.payload.as_ref().map_or(0, Vec::len);
                ActionResult {
                    status: ExecutionStatus::Success,
                    output: Some(
                        format!(
                            "job={} state={} payload_bytes={payload_len}",
                            ctx.job_id, ctx.state
                        )
                        .into_bytes(),
                    ),
                }
            }
            other => ActionResult {
                status: ExecutionStatus::Failed,
                output: Some(format!("unknown action: {other}").into_bytes()),
            },
        }
    }

    fn invoke_guard(name: String, ctx: GuardContext) -> bool {
        match name.as_str() {
            "allow" => ctx.event == "approve" && ctx.to_state == "done",
            "allow_approve" => ctx.event == "approve",
            _ => false,
        }
    }

    fn aggregate(name: String, results: Vec<NodeResult>) -> AggregateDecision {
        let success_count = results
            .iter()
            .filter(|result| result.status == ExecutionStatus::Success)
            .count();

        match name.as_str() {
            // 提供一个可预测的聚合策略，方便宿主侧断言 fan-out 返回值是否被正确解码。
            "pick-success" | "pick_success" if success_count > 0 => AggregateDecision {
                event: "done".to_string(),
                context_patch: Some(format!("success_count={success_count}").into_bytes()),
            },
            "pick-success" | "pick_success" => AggregateDecision {
                event: "retry".to_string(),
                context_patch: Some(b"success_count=0".to_vec()),
            },
            _ => AggregateDecision {
                event: "fallback".to_string(),
                context_patch: None,
            },
        }
    }
}

impl From<ManifestTemplate> for FlowManifest {
    fn from(value: ManifestTemplate) -> Self {
        Self {
            id: value.id,
            host_world: value.world.unwrap_or(FlowWorldTemplate::Sandbox).into(),
            states: value.states.into_iter().map(Into::into).collect(),
            transitions: value.transitions.into_iter().map(Into::into).collect(),
            initial_state: value.initial_state,
            actions: value.actions.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<StateTemplate> for StateDef {
    fn from(value: StateTemplate) -> Self {
        Self {
            name: value.name,
            kind: value.kind.into(),
            on_enter: value.on_enter,
            on_exit: value.on_exit,
            subprocess: value.subprocess.map(Into::into),
        }
    }
}

impl From<StateKindTemplate> for StateKind {
    fn from(value: StateKindTemplate) -> Self {
        match value {
            StateKindTemplate::Normal => Self::Normal,
            StateKindTemplate::Terminal => Self::Terminal,
            StateKindTemplate::Fork => Self::Fork,
            StateKindTemplate::Join => Self::Join,
            StateKindTemplate::Subprocess => Self::Subprocess,
        }
    }
}

impl From<SubprocessTemplate> for SubprocessDef {
    fn from(value: SubprocessTemplate) -> Self {
        Self {
            flow_id: value.flow_id,
            completion_event: value.completion_event,
        }
    }
}

impl From<TransitionTemplate> for TransitionDef {
    fn from(value: TransitionTemplate) -> Self {
        Self {
            from: value.from,
            to: value.to,
            event: value.event,
            guard: value.guard,
            action: value.action,
            timeout: value.timeout.map(Into::into),
        }
    }
}

impl From<TimeoutTemplate> for TimeoutDef {
    fn from(value: TimeoutTemplate) -> Self {
        Self {
            duration_ms: value.duration_ms,
            timeout_event: value.timeout_event,
        }
    }
}

impl From<ActionTemplate> for ActionDef {
    fn from(value: ActionTemplate) -> Self {
        Self {
            name: value.name,
            dispatch: value.dispatch.into(),
            capabilities: value.capabilities.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ActionCapabilityTemplate> for ActionCapability {
    fn from(value: ActionCapabilityTemplate) -> Self {
        match value {
            ActionCapabilityTemplate::Network => Self::Network,
            ActionCapabilityTemplate::Storage => Self::Storage,
        }
    }
}

impl From<DispatchTemplate> for DispatchMode {
    fn from(value: DispatchTemplate) -> Self {
        match value {
            DispatchTemplate::Local => Self::Local,
            DispatchTemplate::Remote => Self::Remote,
            DispatchTemplate::FanOut(config) => Self::FanOut(config.into()),
        }
    }
}

impl From<FlowWorldTemplate> for FlowWorld {
    fn from(value: FlowWorldTemplate) -> Self {
        match value {
            FlowWorldTemplate::Sandbox => Self::Sandbox,
            FlowWorldTemplate::Network => Self::Network,
            FlowWorldTemplate::Storage => Self::Storage,
            FlowWorldTemplate::Full => Self::Full,
        }
    }
}

impl From<FanOutTemplate> for FanOutConfig {
    fn from(value: FanOutTemplate) -> Self {
        Self {
            strategy: value.strategy.into(),
            aggregator: value.aggregator,
            timeout_ms: value.timeout_ms,
            min_success: value.min_success,
        }
    }
}

impl From<FanOutStrategyTemplate> for FanOutStrategy {
    fn from(value: FanOutStrategyTemplate) -> Self {
        match value {
            FanOutStrategyTemplate::All => Self::All,
            FanOutStrategyTemplate::Count(count) => Self::Count(count),
            FanOutStrategyTemplate::Tagged(tags) => Self::Tagged(tags),
        }
    }
}

export!(FlowComponent);
