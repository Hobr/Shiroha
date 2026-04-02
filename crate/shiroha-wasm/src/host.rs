//! WASM Host-Guest 桥接层
//!
//! [`WasmHost`] 负责实例化 WASM 模块并调用其导出函数。
//! 定义了 host 与 guest 之间的数据交换类型（ActionContext, GuardContext）。
//!
//! ## MVP 状态
//!
//! 当前 API 接口已完整定义，但实际的 host-guest 内存协议尚未实现。
//! 所有调用方法返回"未实现"错误。待有测试 WASM 模块后补全：
//! - 通过线性内存交换 JSON 序列化数据
//! - 调用 WASM 导出函数并读取返回值

use serde::{Deserialize, Serialize};

use shiroha_core::flow::FlowManifest;
use shiroha_core::job::{ActionResult, AggregateDecision, NodeResult};

use crate::error::WasmError;

/// Action 执行上下文，传入 WASM guest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionContext {
    pub job_id: String,
    pub state: String,
    pub payload: Option<Vec<u8>>,
}

/// Guard 评估上下文，传入 WASM guest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardContext {
    pub job_id: String,
    pub from_state: String,
    pub to_state: String,
    pub event: String,
    pub payload: Option<Vec<u8>>,
}

/// WASM 模块的 host 端代理
///
/// 每个实例对应一个已加载的 WASM 模块。
/// 通过调用模块导出的函数实现 manifest 提取、action 执行、guard 评估和结果聚合。
pub struct WasmHost {
    _engine: wasmtime::Engine,
    _module: wasmtime::Module,
}

impl WasmHost {
    pub fn new(engine: &wasmtime::Engine, module: &wasmtime::Module) -> Result<Self, WasmError> {
        Ok(Self {
            _engine: engine.clone(),
            _module: module.clone(),
        })
    }

    /// 从 WASM 模块提取状态机拓扑清单
    ///
    /// TODO: 实例化模块，调用 `get_manifest` 导出，从线性内存读取 JSON 并反序列化
    pub fn get_manifest(&mut self) -> Result<FlowManifest, WasmError> {
        Err(WasmError::Execution(
            "WASM host not yet implemented — use JSON manifest loading for MVP".into(),
        ))
    }

    /// 调用 WASM 模块中的 action 函数
    ///
    /// TODO: 将 ctx 序列化为 JSON 写入线性内存，调用导出函数，读取结果
    pub fn invoke_action(
        &mut self,
        _name: &str,
        _ctx: ActionContext,
    ) -> Result<ActionResult, WasmError> {
        Err(WasmError::Execution(
            "WASM action invocation not yet implemented".into(),
        ))
    }

    /// 评估 guard 条件
    ///
    /// TODO: 调用 `invoke_guard` 导出
    pub fn invoke_guard(&mut self, _name: &str, _ctx: GuardContext) -> Result<bool, WasmError> {
        Err(WasmError::Execution(
            "WASM guard invocation not yet implemented".into(),
        ))
    }

    /// 聚合多 Node 的执行结果
    ///
    /// TODO: 调用 `aggregate` 导出
    pub fn aggregate(
        &mut self,
        _name: &str,
        _results: &[NodeResult],
    ) -> Result<AggregateDecision, WasmError> {
        Err(WasmError::Execution(
            "WASM aggregation not yet implemented".into(),
        ))
    }
}
