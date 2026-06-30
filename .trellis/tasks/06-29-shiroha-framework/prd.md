# Shiroha 框架（父任务）

## Goal

一个以 WebAssembly 为一等公民、围绕有限状态机（FSM）构建的可扩展工作流编排引擎。Rust 实现，分三层：

1. **第一层 状态机核心**：高性能、功能强大的状态机；提供 IR adapter 作为状态机定义来源，优先实现 WASM Component Model adapter（运行时读取 WASM 内声明的状态机结构，而非在 WASM 内运行状态机），文件 adapter（JSON/TOML…）为后期功能。
2. **第二层 分布调度器**：将状态机里被指定需分发到不同节点执行的动作/任务分发到无状态节点，返回聚合结果。
3. **第三层 控制器**：可被 GUI/CLI/Web 引入以实现对任务的管理与控制；集成任务管理 / OpenTelemetry 等；含简单的安全校验。非框架直接关联的功能由 GUI/CLI/Web 端单独实现。

WASM 一等公民原则：状态机定义 / action / callback 可写在 WASM 内；聚合策略可用 WASM 定义；action 扩展可用 WASM 开发。WASM 支持 WASI；未来实现 capability 权限管理；创建状态机 task 时需进行授权。

## 跨层原则（所有子任务共享）

- **adapter vs plugin 分层**：adapter 对接「状态机定义」（IR 来源：WASM Component Model / 文件）。plugin 是通用**框架扩展点系统**，一个插件可注册一项或多项能力面：action func（提供 action 实现源，如 http func / bash func…）、middleware（横切关注点：日志/监控/追踪）、aggregation-strategy（聚合策略，第二层）、transport（分布式协议：rpc/p2p/消息服务…，第二层）、**adapter**（扩展状态机定义来源，用户可自定义 IR 适配器）、以及其他未来框架能力。action/callback 选用 WASM，是状态机定义里 action/callback 的「可选类型之一」，与 adapter/plugin 不同层。
- **WASM 边界**：状态机结构在 WASM 内声明、运行时读取（不是把状态机引擎塞进 WASM 运行）；action/callback 在 WASM 内执行。
- **控制面边界**：sctl（CLI）/ Web / GUI 是 engine 的**消费者**，通过 gRPC 控制面调用，绝不绕过直达 engine 内部。控制面 service 经 trait 接缝（Adapter/Authorizer/TaskManager）操作，传输层 auth（鉴调用方）与 capability authz（鉴 task 能力）两层分离。Web/GUI 非框架直接关联，接入方式自选。
- **capability 授权时机**：创建状态机 task 时做 capability 解析与校验，影响 controller 与状态机层的接口契约（未来项，但需在 MVP 预留 hook）。

## Confirmed Facts（来自仓库 inspection）

- Rust edition 2024，toolchain 1.96.0，MSRV 1.95.0；workspace resolver 3，当前 members 为空（greenfield）。
- 依赖已选型：wasmtime 46 + wasmtime-wasi(p3)；tokio(full) + tokio-util + async-trait；clap + clap_complete；tonic + prost（gRPC）；reqwest(rustls)；config 0.15；thiserror + anyhow；shadow-rs。
- WASM 目标：`wasm32-wasip2`（Component Model）；plugin/action 编译目标同为 wasm32-wasip2。
- 规划二进制（justfile）：`shirohad`（守护进程，承载状态机/调度）、`sctl`（控制 CLI，第三层控制器）。
- release profile：LTO + strip + opt-level 3 + codegen-units 1 + panic=abort（性能优先）。
- 许可 GPL-3.0-only；仓库 github.com/Hobr/Shiroha。
- 工具链：cargo-deny、cargo-nextest、cargo-llvm-cov、pre-commit、nix flake。

## Child Tasks

- `06-29-layer1-statemachine-wasm-adapter` — 第一层 MVP：状态机核心 + WASM Component Model adapter。

## Acceptance Criteria（父任务，跨子任务集成层）

- [ ] 三层接口契约可独立编译、各自可测。
- [ ] 跨层 principles（adapter/plugin 分层、WASM 边界、capability hook）在各子任务 design.md 中一致体现。
- [ ] 集成时一个端到端示例（定义→创建 task→执行→（未来）分发→控制）跑通（最终集成验收）。

## Out of Scope（父任务）

- 文件 adapter（JSON/TOML）具体实现（第一层子任务占位，后期实现）。
- 第二/三层具体实现（各自子任务）。
