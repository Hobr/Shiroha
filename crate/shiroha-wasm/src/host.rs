//! WASM Host-Guest 桥接层
//!
//! [`WasmHost`] 按需实例化 core WASM module，并通过线性内存交换 JSON 数据。
//! Phase 1 约定 guest 导出以下符号：
//! - `memory`: 线性内存
//! - `alloc(len: i32) -> i32`: 为 host 写入入参分配缓冲区
//! - `get-manifest() -> i64`
//! - `invoke-action(name_ptr, name_len, ctx_ptr, ctx_len) -> i64`
//! - `invoke-guard(name_ptr, name_len, ctx_ptr, ctx_len) -> i32`
//! - `aggregate(name_ptr, name_len, results_ptr, results_len) -> i64`
//!
//! 其中 `i64` 返回值编码为 `(ptr << 32) | len`，host 读取对应字节并反序列化。

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use shiroha_core::flow::FlowManifest;
use shiroha_core::job::{ActionResult, AggregateDecision, NodeResult};

use crate::error::WasmError;

const DEFAULT_FUEL: u64 = 1_000_000;
const GET_MANIFEST_EXPORTS: &[&str] = &["get-manifest", "get_manifest"];
const INVOKE_ACTION_EXPORTS: &[&str] = &["invoke-action", "invoke_action"];
const INVOKE_GUARD_EXPORTS: &[&str] = &["invoke-guard", "invoke_guard"];
const AGGREGATE_EXPORTS: &[&str] = &["aggregate"];

type PackedBytesResult = i64;
type JsonCallParams = (i32, i32, i32, i32);
type PackedBytesFunc = wasmtime::TypedFunc<JsonCallParams, PackedBytesResult>;
type GuardFunc = wasmtime::TypedFunc<JsonCallParams, i32>;

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

struct GuestInstance {
    store: wasmtime::Store<()>,
    instance: wasmtime::Instance,
    memory: wasmtime::Memory,
    alloc: wasmtime::TypedFunc<i32, i32>,
}

impl GuestInstance {
    fn new(engine: &wasmtime::Engine, module: &wasmtime::Module) -> Result<Self, WasmError> {
        let mut store = wasmtime::Store::new(engine, ());
        store
            .set_fuel(DEFAULT_FUEL)
            .map_err(|e| WasmError::Instantiation(e.to_string()))?;

        let instance = wasmtime::Instance::new(&mut store, module, &[])
            .map_err(|e| WasmError::Instantiation(e.to_string()))?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| WasmError::Memory("guest module must export `memory`".into()))?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .map_err(|e| WasmError::Instantiation(format!("missing `alloc` export: {e}")))?;

        Ok(Self {
            store,
            instance,
            memory,
            alloc,
        })
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(i32, i32), WasmError> {
        let len = i32::try_from(bytes.len())
            .map_err(|_| WasmError::Memory("payload is larger than guest ABI allows".into()))?;
        let ptr = self
            .alloc
            .call(&mut self.store, len)
            .map_err(|e| WasmError::Memory(format!("guest alloc failed: {e}")))?;

        if ptr < 0 {
            return Err(WasmError::Memory(
                "guest alloc returned negative pointer".into(),
            ));
        }

        self.memory
            .write(&mut self.store, ptr as usize, bytes)
            .map_err(|e| WasmError::Memory(format!("failed to write guest memory: {e}")))?;

        Ok((ptr, len))
    }

    fn read_bytes(&mut self, packed: i64) -> Result<Vec<u8>, WasmError> {
        let (ptr, len) = unpack_ptr_len(packed)?;
        if len == 0 {
            return Ok(Vec::new());
        }

        let mut buf = vec![0u8; len];
        self.memory
            .read(&self.store, ptr, &mut buf)
            .map_err(|e| WasmError::Memory(format!("failed to read guest memory: {e}")))?;
        Ok(buf)
    }
}

fn unpack_ptr_len(packed: i64) -> Result<(usize, usize), WasmError> {
    let raw = packed as u64;
    let ptr = (raw >> 32) as u32;
    let len = raw as u32;
    Ok((ptr as usize, len as usize))
}

fn deserialize_json<T: DeserializeOwned>(bytes: Vec<u8>) -> Result<T, WasmError> {
    serde_json::from_slice(&bytes).map_err(|e| WasmError::Serialization(e.to_string()))
}

fn serialize_json<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, WasmError> {
    serde_json::to_vec(value).map_err(|e| WasmError::Serialization(e.to_string()))
}

fn get_typed_func_0_i64(
    store: &mut wasmtime::Store<()>,
    instance: &wasmtime::Instance,
    export_names: &[&str],
) -> Result<wasmtime::TypedFunc<(), PackedBytesResult>, WasmError> {
    for &name in export_names {
        if let Ok(func) = instance.get_typed_func::<(), i64>(&mut *store, name) {
            return Ok(func);
        }
    }

    Err(WasmError::Instantiation(format!(
        "missing export: one of {}",
        export_names.join(", ")
    )))
}

fn get_typed_func_4_i64(
    store: &mut wasmtime::Store<()>,
    instance: &wasmtime::Instance,
    export_names: &[&str],
) -> Result<PackedBytesFunc, WasmError> {
    for &name in export_names {
        if let Ok(func) =
            instance.get_typed_func::<JsonCallParams, PackedBytesResult>(&mut *store, name)
        {
            return Ok(func);
        }
    }

    Err(WasmError::Instantiation(format!(
        "missing export: one of {}",
        export_names.join(", ")
    )))
}

fn get_typed_func_4_i32(
    store: &mut wasmtime::Store<()>,
    instance: &wasmtime::Instance,
    export_names: &[&str],
) -> Result<GuardFunc, WasmError> {
    for &name in export_names {
        if let Ok(func) = instance.get_typed_func::<JsonCallParams, i32>(&mut *store, name) {
            return Ok(func);
        }
    }

    Err(WasmError::Instantiation(format!(
        "missing export: one of {}",
        export_names.join(", ")
    )))
}

/// WASM 模块的 host 端代理
///
/// 每个实例对应一个已加载的 WASM 模块。
/// 通过调用模块导出的函数实现 manifest 提取、action 执行、guard 评估和结果聚合。
pub struct WasmHost {
    engine: wasmtime::Engine,
    module: wasmtime::Module,
}

impl WasmHost {
    pub fn new(engine: &wasmtime::Engine, module: &wasmtime::Module) -> Result<Self, WasmError> {
        Ok(Self {
            engine: engine.clone(),
            module: module.clone(),
        })
    }

    fn guest(&self) -> Result<GuestInstance, WasmError> {
        GuestInstance::new(&self.engine, &self.module)
    }

    /// 从 WASM 模块提取状态机拓扑清单
    pub fn get_manifest(&mut self) -> Result<FlowManifest, WasmError> {
        let mut guest = self.guest()?;
        let get_manifest =
            get_typed_func_0_i64(&mut guest.store, &guest.instance, GET_MANIFEST_EXPORTS)?;
        let packed = get_manifest
            .call(&mut guest.store, ())
            .map_err(|e| WasmError::Execution(e.to_string()))?;
        deserialize_json(guest.read_bytes(packed)?)
    }

    /// 调用 WASM 模块中的 action 函数
    pub fn invoke_action(
        &mut self,
        name: &str,
        ctx: ActionContext,
    ) -> Result<ActionResult, WasmError> {
        let mut guest = self.guest()?;
        let invoke_action =
            get_typed_func_4_i64(&mut guest.store, &guest.instance, INVOKE_ACTION_EXPORTS)?;
        let (name_ptr, name_len) = guest.write_bytes(name.as_bytes())?;
        let ctx_bytes = serialize_json(&ctx)?;
        let (ctx_ptr, ctx_len) = guest.write_bytes(&ctx_bytes)?;

        let packed = invoke_action
            .call(&mut guest.store, (name_ptr, name_len, ctx_ptr, ctx_len))
            .map_err(|e| WasmError::Execution(e.to_string()))?;
        deserialize_json(guest.read_bytes(packed)?)
    }

    /// 评估 guard 条件
    pub fn invoke_guard(&mut self, name: &str, ctx: GuardContext) -> Result<bool, WasmError> {
        let mut guest = self.guest()?;
        let invoke_guard =
            get_typed_func_4_i32(&mut guest.store, &guest.instance, INVOKE_GUARD_EXPORTS)?;
        let (name_ptr, name_len) = guest.write_bytes(name.as_bytes())?;
        let ctx_bytes = serialize_json(&ctx)?;
        let (ctx_ptr, ctx_len) = guest.write_bytes(&ctx_bytes)?;

        let accepted = invoke_guard
            .call(&mut guest.store, (name_ptr, name_len, ctx_ptr, ctx_len))
            .map_err(|e| WasmError::Execution(e.to_string()))?;
        Ok(accepted != 0)
    }

    /// 聚合多 Node 的执行结果
    pub fn aggregate(
        &mut self,
        name: &str,
        results: &[NodeResult],
    ) -> Result<AggregateDecision, WasmError> {
        let mut guest = self.guest()?;
        let aggregate = get_typed_func_4_i64(&mut guest.store, &guest.instance, AGGREGATE_EXPORTS)?;
        let (name_ptr, name_len) = guest.write_bytes(name.as_bytes())?;
        let result_bytes = serialize_json(results)?;
        let (results_ptr, results_len) = guest.write_bytes(&result_bytes)?;

        let packed = aggregate
            .call(
                &mut guest.store,
                (name_ptr, name_len, results_ptr, results_len),
            )
            .map_err(|e| WasmError::Execution(e.to_string()))?;
        deserialize_json(guest.read_bytes(packed)?)
    }
}

#[cfg(test)]
mod tests {
    use shiroha_core::job::ExecutionStatus;

    use super::*;
    use crate::runtime::WasmRuntime;

    const TEST_MODULE: &str = r#"
        (module
          (memory (export "memory") 1)
          (global $heap (mut i32) (i32.const 2048))

          (data (i32.const 0) "{\"id\":\"demo\",\"states\":[{\"name\":\"idle\",\"kind\":\"normal\"},{\"name\":\"done\",\"kind\":\"terminal\"}],\"transitions\":[{\"from\":\"idle\",\"to\":\"done\",\"event\":\"finish\",\"guard\":\"allow\",\"action\":\"ship\"}],\"initial_state\":\"idle\",\"actions\":[{\"name\":\"ship\",\"dispatch\":\"local\"}]}")
          (data (i32.const 320) "{\"status\":\"success\",\"output\":[79,75]}")
          (data (i32.const 384) "{\"status\":\"failed\"}")
          (data (i32.const 448) "{\"event\":\"done\",\"context_patch\":[1,2]}")
          (data (i32.const 512) "{\"event\":\"fallback\"}")

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
            i32.const 253
            call $pack)

          (func (export "invoke-action") (param $name_ptr i32) (param $name_len i32) (param $ctx_ptr i32) (param $ctx_len i32) (result i64)
            local.get $ctx_ptr
            i32.load8_u
            drop
            local.get $name_ptr
            i32.load8_u
            i32.const 115
            i32.eq
            if (result i64)
              i32.const 320
              i32.const 37
              call $pack
            else
              i32.const 384
              i32.const 19
              call $pack
            end)

          (func (export "invoke-guard") (param $name_ptr i32) (param $name_len i32) (param $ctx_ptr i32) (param $ctx_len i32) (result i32)
            local.get $ctx_ptr
            i32.load8_u
            drop
            local.get $name_ptr
            i32.load8_u
            i32.const 97
            i32.eq)

          (func (export "aggregate") (param $name_ptr i32) (param $name_len i32) (param $results_ptr i32) (param $results_len i32) (result i64)
            local.get $results_ptr
            i32.load8_u
            drop
            local.get $name_ptr
            i32.load8_u
            i32.const 109
            i32.eq
            if (result i64)
              i32.const 448
              i32.const 38
              call $pack
            else
              i32.const 512
              i32.const 20
              call $pack
            end))
    "#;

    fn host() -> WasmHost {
        let runtime = WasmRuntime::new().expect("runtime");
        let module = runtime.load_module(TEST_MODULE.as_bytes()).expect("module");
        WasmHost::new(runtime.engine(), &module).expect("host")
    }

    #[test]
    fn extracts_manifest_from_guest() {
        let mut host = host();
        let manifest = host.get_manifest().expect("manifest");

        assert_eq!(manifest.id, "demo");
        assert_eq!(manifest.initial_state, "idle");
        assert_eq!(manifest.states.len(), 2);
        assert_eq!(manifest.transitions.len(), 1);
        assert_eq!(manifest.actions.len(), 1);
    }

    #[test]
    fn invokes_action_with_json_context() {
        let mut host = host();
        let result = host
            .invoke_action(
                "ship",
                ActionContext {
                    job_id: "job-1".into(),
                    state: "done".into(),
                    payload: Some(vec![1, 2, 3]),
                },
            )
            .expect("action");

        assert_eq!(result.status, ExecutionStatus::Success);
        assert_eq!(result.output, Some(b"OK".to_vec()));
    }

    #[test]
    fn invokes_guard_with_json_context() {
        let mut host = host();
        let allowed = host
            .invoke_guard(
                "allow",
                GuardContext {
                    job_id: "job-1".into(),
                    from_state: "idle".into(),
                    to_state: "done".into(),
                    event: "finish".into(),
                    payload: None,
                },
            )
            .expect("guard");
        let denied = host
            .invoke_guard(
                "deny",
                GuardContext {
                    job_id: "job-1".into(),
                    from_state: "idle".into(),
                    to_state: "done".into(),
                    event: "finish".into(),
                    payload: None,
                },
            )
            .expect("guard");

        assert!(allowed);
        assert!(!denied);
    }

    #[test]
    fn aggregates_node_results() {
        let mut host = host();
        let decision = host
            .aggregate(
                "merge",
                &[NodeResult {
                    node_id: "node-a".into(),
                    status: ExecutionStatus::Success,
                    output: Some(vec![1, 2]),
                }],
            )
            .expect("aggregate");

        assert_eq!(decision.event, "done");
        assert_eq!(decision.context_patch, Some(vec![1, 2]));
    }
}
