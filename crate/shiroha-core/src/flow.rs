//! 状态机定义（Flow）
//!
//! Flow 是一个完整的状态机描述，对应一个 WASM 模块。
//! 包含状态（State）、转移（Transition）、动作（Action）等定义。
//! 通过 WASM 导出的 `get-manifest` 函数获取。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 状态机拓扑清单，由 WASM 模块的 `get-manifest` 导出返回
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowManifest {
    /// guest 自描述的 flow 标识；部署时平台侧也会额外维护自己的注册键。
    pub id: String,
    /// guest 声明所需的 capability world，部署时会与组件实际 imports 做一致性校验。
    pub world: FlowWorld,
    /// 完整状态集合，和 `transitions` 一起构成静态拓扑快照。
    pub states: Vec<StateDef>,
    pub transitions: Vec<TransitionDef>,
    /// 状态机的起始状态名，必须存在于 `states` 中
    pub initial_state: String,
    /// Action 元信息注册表，声明每个 action 的分发策略
    pub actions: Vec<ActionDef>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlowWorld {
    Sandbox,
    Network,
    Storage,
    Full,
}

/// 状态节点定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDef {
    pub name: String,
    pub kind: StateKind,
    /// 声明式进入钩子；具体何时触发由控制面执行链路决定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_enter: Option<String>,
    /// 声明式离开钩子；通常与转移动作分开建模。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_exit: Option<String>,
    /// 子流程配置，仅在 `kind = Subprocess` 时有效
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subprocess: Option<SubprocessDef>,
}

/// 状态类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateKind {
    Normal,
    /// 终态，到达后 Job 标记为 Completed
    Terminal,
    /// 分叉节点（并行分支）
    Fork,
    /// 汇合节点（并行合并）
    Join,
    /// 子流程节点，进入时自动创建子 Job
    Subprocess,
}

/// 子流程配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubprocessDef {
    /// 要启动的子 Flow ID
    pub flow_id: String,
    /// 子 Flow 完成后注入主 Job 的事件名
    pub completion_event: String,
}

/// 状态转移边定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionDef {
    pub from: String,
    pub to: String,
    /// 触发此转移的事件名
    pub event: String,
    /// 转移前的守卫条件函数名（必须为纯函数）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guard: Option<String>,
    /// 转移时执行的 action 函数名
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// 超时配置：在源状态停留超过指定时间后自动触发超时事件
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<TimeoutDef>,
}

/// 转移超时定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeoutDef {
    pub duration_ms: u64,
    /// 超时后自动注入的事件名
    pub timeout_event: String,
}

/// Action 元信息，声明分发策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionDef {
    pub name: String,
    pub dispatch: DispatchMode,
}

/// Action 分发模式
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchMode {
    /// Controller 本地执行
    Local,
    /// 分发到单个 Node 执行；standalone 模式下通常退化为本地调用。
    Remote,
    /// 分发到多个 Node 并行执行，聚合结果后决定状态转移
    FanOut(FanOutConfig),
}

/// Fan-out 并行分发配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanOutConfig {
    pub strategy: FanOutStrategy,
    /// WASM 内定义的聚合函数名
    pub aggregator: String,
    /// 整体超时（毫秒），超时后用已收集的结果聚合
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// 最少成功数，达到即可提前聚合
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_success: Option<u32>,
}

/// Fan-out 节点选择策略
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FanOutStrategy {
    /// 所有可用 Node
    All,
    /// 指定数量的 Node
    Count(u32),
    /// 带指定标签的 Node
    Tagged(Vec<String>),
}

/// 已注册的 Flow，包含版本与 WASM 模块引用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowRegistration {
    pub flow_id: String,
    /// 部署版本号（UUIDv7），每次部署递增
    pub version: Uuid,
    /// 部署时冻结下来的 manifest 快照，保证旧 Job 可以继续按旧版拓扑运行。
    pub manifest: FlowManifest,
    /// WASM 模块的内容哈希，用于 Node 端缓存
    pub wasm_hash: String,
}
