# v0.2.5+ Integration Testing: WASM Component + End-to-End Verification

> 父任务：`06-29-layer1-statemachine-wasm-adapter`

## Goal

验证 v0.2.5 声称的"完整 WASM action 执行"确实能端到端工作。当前实现编译通过但**未经过集成测试**，存在 wasmtime API 误用、WIT 接口不匹配等潜在风险。本任务交付：

1. **真实 WASM component 示例**（`examples/sm-example`）— 实现完整 WIT 接口并编译为 wasm32-wasip2
2. **集成测试**（`crates/wasm/tests/integration.rs`）— 加载 component → 构建 IR → 实例化 task → 注入事件 → 验证 action 执行
3. **（可选）最小 shirohad CLI** — 交互式加载状态机并触发事件（优先级次之）

## Problem Statement

v0.2.5 commit bf67006 实现了 `WasmActionInvoker` 和 `WasmAdapter`，但：

- ❌ 没有可加载的真实 WASM component（`sm-example` 仅占位注释）
- ❌ 没有集成测试验证 `WasmAdapter::load()` → `TaskManager::create()` → action 执行的完整流程
- ❌ `shirohad` 仍是 "Hello, world!"，无法演示端到端功能
- ⚠️ **未验证的实现 ≠ 完成的功能** — 在继续 v0.3.0 plugin 架构前应先验证基础能力

## Requirements

### R1: 真实 WASM Component（`examples/sm-example`）

- R1.1 实现 `shiroha:sm/state-machine` WIT 接口的所有导出：
  - `states() -> list<state-def>`
  - `transitions() -> list<transition-def>`
  - `actions() -> list<action-def>`
  - `initial-state() -> string`
  - `events() -> list<event-def>`
- R1.2 至少定义一个**可执行的 action**（如 `log-message` 或 `increment-counter`），导出为独立函数
- R1.3 使用 `wit-bindgen` 生成 guest bindings
- R1.4 编译为 `wasm32-wasip2` target
- R1.5 简单状态机拓扑：2-3 个状态，至少 1 个迁移触发 action

### R2: 集成测试（`crates/wasm/tests/integration.rs`）

- R2.1 **Load phase**: `WasmAdapter::load("sm-example.wasm")` → `StateMachineDef` IR
  - 验证 states/transitions/actions 正确解析
  - 验证 initial state 存在
- R2.2 **Instantiate phase**: 从 IR 创建 task → `TaskManager::create()`
  - 验证 task 初始状态正确
- R2.3 **Execute phase**: 注入事件 → 触发迁移 → 执行 action
  - 验证状态迁移正确
  - 验证 action 被调用（通过 host import `host.log` 或返回值）
- R2.4 测试通过 `cargo test --test integration`

### R3: Host Import Verification

- R3.1 实现 `host.log(message: string)` host function
- R3.2 WASM action 调用 `host.log()` 输出到 test capture
- R3.3 集成测试断言日志输出包含预期消息

### R4: （可选）最小 shirohad CLI

优先级次之，可推迟到 v0.3.0。

- R4.1 接受 `--component <path>` 参数加载 WASM component
- R4.2 输出加载的状态机结构（states/transitions）
- R4.3 交互式 REPL：输入事件名触发迁移
- R4.4 输出当前状态 + action 执行日志

## Acceptance Criteria

### 必须完成

- [ ] `examples/sm-example/src/lib.rs` 实现完整 WIT 接口（非占位）
- [ ] `examples/sm-example/Cargo.toml` 包含 `wit-bindgen` 依赖和正确 `crate-type`
- [ ] `cargo build --target wasm32-wasip2 -p shiroha-sm-example` 成功生成 `.wasm` 文件
- [ ] `crates/wasm/tests/integration.rs` 包含至少 3 个测试：
  - `test_load_wasm_component()` — 加载 component 并验证 IR
  - `test_instantiate_task()` — 从 IR 创建 task
  - `test_execute_action()` — 触发迁移并验证 action 执行
- [ ] `cargo test --test integration` 通过
- [ ] Host import `host.log()` 可被 WASM action 调用，测试中可捕获输出

### 可选（推迟）

- [ ] `bin/shirohad/Cargo.toml` 包含 `shiroha-wasm` / `shiroha-engine` 依赖
- [ ] `bin/shirohad/src/main.rs` 实现 CLI 参数解析和交互式 REPL
- [ ] 可运行 `shirohad --component examples/sm-example/target/wasm32-wasip2/debug/shiroha_sm_example.wasm`

## Out of Scope

- 复杂状态机拓扑（嵌套状态、history）— 集成测试只需验证基础 load → instantiate → execute 流程
- 多组件加载 — 单个 component 足够验证机制
- 性能测试 — 功能正确性优先
- CLI 错误处理和用户体验 — 最小实现即可（如果做 R4）

## Technical Notes

### 示例状态机拓扑建议

```
States: Idle, Processing, Done
Events: start, finish
Transitions:
  - Idle --[start]--> Processing (action: log-message "Started processing")
  - Processing --[finish]--> Done (action: log-message "Finished processing")
Initial: Idle
```

### WIT 接口位置

- `wit/state-machine.wit` — 已定义完整接口
- Component 需 `export shiroha:sm/state-machine` world

### 集成测试依赖

`crates/wasm/Cargo.toml` 需添加 `[dev-dependencies]`：
```toml
[dev-dependencies]
shiroha-engine = { path = "../engine" }
tokio = { version = "1", features = ["rt", "macros"] }
```

### Host Import 实现

参考 `crates/wasm/src/invoker.rs` 已有的 `HostImpl`，添加：
```rust
impl shiroha::sm::host::Host for HostImpl {
    fn log(&mut self, message: String) {
        tracing::info!("WASM action log: {}", message);
    }
}
```

## Dependencies

- **Blocks**: v0.3.0 plugin 架构实现 — 应先验证 v0.2.5 基础能力再继续
- **Blocked by**: 无 — v0.2.5 实现已提交，立即可开始

## Risks

- **Medium**: `wit-bindgen` macro 展开可能需要调整编译配置（已知 wasmtime 46.x bindgen 有特殊要求）
- **Low**: 集成测试需要正确设置 wasmtime `Engine` 和 `Store`，可能需参考 wasmtime 文档
- **Low**: wasm32-wasip2 target 需 `rustup target add wasm32-wasip2`（文档应说明）

## Success Metrics

- `cargo test --test integration` 全部通过
- 可加载真实 WASM component 并正确解析为 IR
- 可从 IR 实例化 task 并触发状态迁移
- WASM action 能调用 host import 并产生可观测的副作用（日志输出）
- 所有 v0.2.5 acceptance criteria 标记为已完成（PRD 更新）
