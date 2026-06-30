# Implement — 第一层状态机核心与 WASM Component Model adapter

> 依据 `prd.md`（D1–D5）与 `design.md`。有序清单；每步附验证命令。

## 版本路线图（MVP 拆分）

MVP 拆为 4 个可独立验证的版本。原则：每版可独立编译/测试、有清晰价值增量、风险点隔离在单一版本内。

| 版本 | 价值增量 | 范围 | 依赖 |
|---|---|---|---|
| **v0.1.0** 状态机核心 | 交付可用的层级 HSM 引擎（纯 Rust，无 WASM） | 阶段 0 工程骨架 + 阶段 2 IR + 阶段 3 HSM 运行时；用 stub `ActionInvoker` 完整验证状态机语义（嵌套/entry-exit/history/guard/RTC/do-activity/Task actor） | 无外部运行时依赖 |
| **v0.2.0** WASM Component Model adapter | WASM 成为一等公民：定义与 action 可在 WASM 内 | 阶段 1 WIT 接口 + 阶段 4 WASM adapter；含风险点验证（wasmtime async）；`examples/sm-example` 端到端跑通 | v0.1.0 |
| **v0.2.5** WASM action 执行 | WASM action 真正可用：完整 invoker 实现 + 端到端验证 | 阶段 4.5 WASM invoker 实现（替换占位）+ 简单示例 action + 集成测试 | v0.2.0 |
| **v0.3.0** plugin 扩展点系统 | 框架可扩展，能力面注册机制就位 | 阶段 5 plugin：`PluginRegistry` + 五能力面 trait（全部留口）+ `ActionInvoker` 接口修改 + `CompositeActionInvoker` 路由 | v0.2.5 |
| **v0.4.0** 守护进程、控制面与多形态集成 | 可运行三形态 shirohad + gRPC 控制面 + sctl + 质量门 | 阶段 6 `shirohad`（full/controller/node feature）+ `shiroha-control`（ShirohaControl + NodeExecutor）+ `sctl` gRPC client + `examples` 完善 + 阶段 7 质量门 | v0.3.0 |

### 版本边界与风险隔离

- **v0.1.0 不依赖 wasmtime**：纯 Rust engine + ir crate，可独立稳定。即使 v0.2.0 的 wasmtime async 风险爆发，v0.1.0 的状态机语义仍可用。
- **v0.2.0 隔离最大风险**：wasmtime 46 component-model async（do-activity `invoke-do` future）成熟度验证在此版本内；若不稳则落回退方案（同步计算 + tokio 可取消包裹），不回退到 v0.1.0。
- **v0.3.0/v0.4.0 无高风险点**：纯集成与扩展，依赖前两版稳定。

### 各版本验收形态

- **v0.1.0**：`just check` + `cargo nextest run -p shiroha-engine` 通过；stub action 驱动完整 HSM 语义。
- **v0.2.5**：`cargo build -p shiroha-sm-example --target wasm32-wasip2` + 端到端集成测试（加载组件→IR→task→事件→**真实 wasm action 执行**）通过；`WasmActionInvoker` 完整实现（非占位）。
- **v0.3.0**：`PluginRegistry` + 五能力面 trait 可编译；`CompositeActionInvoker` 可根据 `ActionKind` 正确路由；单元测试通过（空 registry 查找、路由逻辑）。
- **v0.4.0**：`just build` + `just test` + `just fmt` + `cargo deny check` + `just coverage` 全通过；三形态 shirohad 各自可构建（`--features full/controller/node`）；sctl 经 gRPC 创建 task/发事件/查状态端到端跑通；full 形态本地 node 执行 do-activity 跑通；对照 `prd.md` Acceptance Criteria 逐项勾选。

---

## 阶段 0 — 工程骨架

- [ ] 0.1 workspace 增加 members：`crates/ir`、`crates/engine`、`crates/wasm`、`crates/plugin`、`bin/shirohad`、`bin/sctl`、`examples/sm-example`；`[workspace.dependencies]` 内已有依赖按 crate 拆分引用（tokio/wasmtime/thiserror…）。
- [ ] 0.2 `shiroha-ir` crate 空骨架（lib.rs + Cargo.toml），`just check` 通过。
- [ ] 0.3 验证：`just check`。

## 阶段 1 — WIT 接口（第一类契约，D3）

- [ ] 1.1 建 `wit/state-machine.wit`，落 `design.md §6` 草图：`shiroha:sm` 包，`types`/`definition`/`actions`/`host-interface` 接口，`world state-machine`。
- [ ] 1.2 host 侧绑定：`shiroha-wasm` 用 `wit-bindgen`（或 wasmtime component 绑定）生成 host 绑定。
- [ ] 1.3 guest 侧绑定：`examples/sm-example` 用 `wit-bindgen` guest 模式生成绑定，验证可编译到 wasm32-wasip2。
- [ ] 1.4 验证：`cargo build -p shiroha-sm-example --target wasm32-wasip2`。

## 阶段 2 — IR（`shiroha-ir`）

- [ ] 2.1 落 `design.md §3` 的 `StateMachineDef`/`State`/`Transition`/`ActionRef`/`HistoryConfig`/`GuardRef` 类型；预留 `State::ortho` 字段（标注 `#[allow(dead_code)]`，MVP 不用）。
- [ ] 2.2 构造校验：initial 存在、parent 引用有效、无环嵌套、history 配置合法。
- [ ] 2.3 单元测试：合法/非法 IR 构造。
- [ ] 2.4 验证：`cargo nextest run -p shiroha-ir`。

## 阶段 3 — HSM 运行时核心（`shiroha-engine`）

- [ ] 3.1 状态树构建（嵌套 + LCA 计算，用于迁移的 entry/exit 级联顺序）。
- [ ] 3.2 history 存储：shallow（记录直接子状态）+ deep（记录嵌套活跃路径，MVP 单条路径）。
- [ ] 3.3 RTC 事件循环：mailbox（tokio `mpsc`）→ 取事件 → 选 transition（含 guard 求值）→ 级联 exit/action/entry → 更新配置/历史。
- [ ] 3.4 do-activity 生命周期：进入状态 spawn tokio task，退出时 cancel；完成产生内部完成事件。
- [ ] 3.5 trait 接缝：`Adapter`/`ActionInvoker`/`Plugin`/`Authorizer`（Authorizer 默认 no-op impl）。
- [ ] 3.6 `Task` actor + `TaskHandle`（`send(event)`、`TaskId`）。
- [ ] 3.7 单元测试：用 stub `ActionInvoker` 验证迁移/嵌套/entry-exit/history/guard 阻断/do 取消。
- [ ] 3.8 验证：`cargo nextest run -p shiroha-engine`。

## 阶段 4 — WASM adapter（`shiroha-wasm`，优先交付物）

### v0.2.0: WIT 接口 + 占位结构

- [x] 4.1 `WasmAdapter`：wasmtime `Component` 加载 → 实例化（链接 host-interface）→ 调 `definition.*` 组装 `StateMachineDef`。
- [x] 4.2 `WasmActionInvoker`：`invoke_sync` → 调 `actions.invoke(ctx)`；`invoke_do` → 调 `actions.invoke-do`（component-model async future，await + cancel）。
- [x] 4.3 host import：实现 `host-interface.log`（MVP 仅此）。
- [x] 4.4 **风险验证点（design §8）**：先跑通 wasmtime component-model async 的最小 do-activity 调用 + 取消；若不稳，落回退方案（do-activity 同步计算 + tokio 包裹可取消，长跑交 host plugin）。
- [x] 4.5 集成测试：用 `examples/sm-example` 验证 IR 组装正确。
- [x] 4.6 验证：`cargo nextest run -p shiroha-wasm`。

### v0.2.5: 完整 WASM action 执行（新增阶段）

- [ ] 4.7 实现 `WasmActionInvoker::invoke_sync` 完整逻辑（替换占位）：加载组件 → 调用 `actions.invoke(ctx)` → 返回 `ActionResult`。
- [ ] 4.8 实现 `WasmActionInvoker::invoke_do` 完整逻辑（替换占位）：调用 `actions.invoke-do(ctx)` → tokio 可取消包裹 → 返回 `ActionResult`。
- [ ] 4.9 `examples/sm-example` 实现简单 action（如 log 输出或 counter 递增），验证 host import 可用。
- [ ] 4.10 端到端集成测试：加载组件 → 产 IR → 创建 task → 注入事件 → **真实执行 WASM action** → 验证结果。
- [ ] 4.11 验证：`cargo nextest run -p shiroha-wasm` + 端到端测试通过。

## 阶段 5 — plugin 扩展点系统（`shiroha-plugin`，架构留口）

### v0.3.0: Plugin 架构就位（调整后范围）

- [ ] 5.1 `Plugin` trait + `PluginRegistry` 结构定义（Arc 共享，不可变；typed 各能力面容器：ActionFunc / Middleware / AggregationStrategy / Transport / Adapter）。
- [ ] 5.2 各能力面 trait 定义：**全部仅定义 trait + registry 存取，无内置实现**。
  - [ ] 5.2.1 `ActionFunc` trait（`invoke(ctx) -> Result<ActionResult>`）
  - [ ] 5.2.2 `Middleware` trait（占位签名，无链式调用实现）
  - [ ] 5.2.3 `AggregationStrategy` trait
  - [ ] 5.2.4 `Transport` trait
  - [ ] 5.2.5 `Adapter` trait（复用 engine 已定义的 Adapter trait）
- [ ] 5.3 修改 `ActionInvoker` trait 签名：传递 `ActionRef` 而非仅 name（破坏性变更，需同步修改 `WasmActionInvoker` 占位实现）。
- [ ] 5.4 `CompositeActionInvoker` 实现：按 `ActionRef.kind` 路由（Wasm → wasm invoker；Plugin → registry.action_func）。
- [ ] 5.5 单元测试：空 registry 查找返回 None；路由逻辑正确（Wasm/Plugin 分支）。
- [ ] 5.6 验证：`cargo nextest run -p shiroha-plugin`。

### v0.3.5+: HTTP ActionFunc 实现（推迟）

- [ ] 5.7 内置 http action func（基于 `reqwest`），作为 `ActionFunc` 能力面注册示例。
- [ ] 5.8 `HttpConfig` 结构定义（url / method / headers / body / timeout_secs）。
- [ ] 5.9 错误处理：所有 HTTP 错误（网络/4xx/5xx）统一映射到 `ActionResult::Error`。
- [ ] 5.10 `shirohad` 装配：构建 `PluginRegistry`，注册 HTTP func，传递给 `CompositeActionInvoker`。
- [ ] 5.11 集成测试：HTTP action 经 registry 注册并被 invoker 路由调用。

## 阶段 6 — 守护进程、控制面与示例

- [ ] 6.1 `proto/shiroha.proto`：落 `design.md §12.4` 的 `ShirohaControl` service + `§12.9` 的 `NodeExecutor` service + 所有 message 定义。
- [ ] 6.2 `shiroha-control` crate：tonic-prost-build 生成 stubs；`ShirohaControl` service impl（消费 TaskManager+Adapter+Authorizer，按 §12.5 调用链）；`NodeExecutor` service impl（消费本地 ActionInvoker）；传输层 auth interceptor（no-op）。
- [ ] 6.3 `shirohad` crate：cargo feature 三形态（`full`/`controller`/`node`，见 §12.8）；`full`=controller+本地 node 同进程（node 注册到本进程 controller，do-activity 本地直执行）；`controller`=仅控制面；`node`=注册到指定 controller + 起 NodeExecutor service。
- [ ] 6.4 controller 侧调度逻辑（MVP 极简）：full 优先本地直执行，否则 round-robin 选远端 node 经 `NodeExecutor::ExecuteActivity` 分发。
- [ ] 6.5 `sctl`：clap 子命令（definition load/list, task create/list/send/state/control, node list/register）+ gRPC client 调 shirohad。
- [ ] 6.6 `examples/sm-example`：实现一个完整层级 HSM 组件（含 entry/exit/do/guard/history），供集成测试。
- [ ] 6.7 端到端集成测试：三形态各自构建通过；full 形态 sctl → gRPC → shirohad 加载组件 → 产 IR → 创建 task → 注入事件 → do-activity 本地 node 执行 → 断言全部 acceptance。
- [ ] 6.8 验证：`just check && just test` + 三形态构建命令。

## 阶段 7 — 质量门

- [ ] 7.1 `just fmt`（cargo fmt + pre-commit）。
- [ ] 7.2 `cargo deny check`（许可证/漏洞）。
- [ ] 7.3 `just coverage`，关注 engine 与 wasm crate 覆盖率。
- [ ] 7.4 对照 `prd.md` Acceptance Criteria 逐项勾选。

## 验证命令汇总

```bash
just check          # cargo check --workspace
just test           # cargo nextest run --all-features --run-ignored all
just fmt            # cargo fmt + pre-commit
just coverage       # cargo llvm-cov nextest
cargo deny check    # 许可证/安全
cargo build -p shiroha-sm-example --target wasm32-wasip2   # 组件可编译 (v0.2.0+)
```

## 风险点 / 回滚点

- **R1 wasmtime async（v0.2.0 阶段 4.4）**：最大风险。先单独验证；失败则回退同步 do-activity 包裹方案，记录为已知限制。风险隔离在 v0.2.0，不波及 v0.1.0。
- **R2 WIT 稳定性**：接口在 v0.2.0 阶段 1 定型后尽量不动；若阶段 4 发现不可行，回到阶段 1 调整（不波及 engine，因 engine 只认 IR）。
- **R3 wasm32-wasip2 工具链**：确保 `rust-toolchain.toml` 的 target 与 wasmtime 46 组件模型对齐。
- 回滚点：每阶段独立可测；阶段 2/3（纯 Rust engine）不依赖 wasm，可先行稳定（v0.1.0 范围）。
