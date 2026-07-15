# Shiroha

> A *WebAssembly*-extensible workflow orchestration engine built around *Finite-State Machines*.

Shiroha 是一个可通过 WebAssembly 扩展、以确定性有限状态机为核心的工作流运行时。
v0.1 系列以本地 Rust 库的形式提供：WASM Component 负责定义状态机并实现
Guard、Action 与 Callback，Host 则负责执行顺序、已提交状态、事件队列、校验、
资源限制和诊断。

## v0.1 范围

本仓库已经实现：

- 仅包含一个活动状态的扁平事件驱动 FSM；
- 按声明顺序求值的 Guard，以及固定的 exit -> action -> entry 生命周期语义；
- 由 Host 持有的 Context/State 原子提交和 FIFO 内部事件队列；
- 正常目标与业务失败目标；
- 逻辑超时/取消输入，以及可观测的未处理事件；
- 具有 Adapter 和 Executor 边界、与具体运行时无关的 Core；
- 使用类型化 WIT 调用的 Wasmtime Component Model Adapter；
- 规范 WIT 包、Rust Guest SDK 和 WASIp2 示例 Component；
- 有限的 Epoch/Fuel、墙钟时间、内存、Payload、事件和微步限制；
- 结构化 `tracing` Span；
- 异步 Rust Facade API 和可运行示例。

Controller、无状态 Node、分布式调度器、`sctl`、文本 Adapter、动态插件、Task
授权和可配置的 Capability 策略均属于 v1 前里程碑，而不是 v0.1 中无法执行的占位
API。

## 架构

```text
应用程序
    ↓
shiroha Facade
    ├── shiroha-core             Host IR、校验、FSM 引擎
    └── shiroha-adapter-wasm     Wasmtime 加载器和 Guest Executor
            ↓
      WASM Component
      ├── 状态机定义
      ├── Guard
      ├── Action
      └── Callback
```

Guest 从不运行状态机循环。运行时可以为热路径复用 Guest 内存，但这些内存可以随时
丢弃，也不是工作流状态的权威来源。

## Host 使用方式

```rust,no_run
use shiroha::core::{HostInput, PayloadEnvelope};
use shiroha::{Event, EventName, ShirohaRuntime};

# async fn run(component: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
let runtime = ShirohaRuntime::builder().build()?;
let prepared = runtime.prepare_component(component).await?;
let mut machine = prepared
    .start(PayloadEnvelope::json(br#"{"phase":"idle"}"#.to_vec()))
    .await?;

let report = machine
    .dispatch(HostInput::Event(Event::new(
        EventName::new("begin")?,
        None,
    )))
    .await?;

println!("outcome: {:?}", report.outcome);
println!("snapshot: {:?}", machine.snapshot());
# Ok(())
# }
```

`LocalMachine::dispatch` 需要 `&mut self`，因此如果应用程序没有自行提供 Actor 或
Mutex 边界，同一个实例就无法处理重入或并发事件。

## Guest Component

官方 Rust Guest 目标是 `wasm32-wasip2`。规范合同位于
[`wit/shiroha-machine/world.wit`](wit/shiroha-machine/world.wit)，
`shiroha-guest` 提供生成类型、`MachineGuest` Trait、辅助函数和
`export_machine!` 宏。

构建并检查示例：

```bash
just build-example
just validate-example
```

生成产物位于
`target/components/wasm32-wasip2/debug/example_machine.wasm`。

### 基线 WASI 配置

使用 `wasm32-wasip2` 构建的普通 Rust `std` Component 即使没有显式使用 WASI，
也会声明标准 WASI 0.2 Import。v0.1 通过 `wasmtime-wasi` 链接这些标准接口。

每个 Store 都从 `WasiCtxBuilder::new()` 开始构建：

- stdin 关闭，stdout/stderr 指向 Sink；
- 不继承 Host 环境变量或命令行参数；
- 不预打开任何文件系统目录；
- 默认拒绝套接字地址和名称解析。

Allowlist 精确对应固定版本 Wasmtime 46.0.1 Linker 注册的稳定 Preview 2 接口，
最高支持到 0.2.12。已识别 WASI 家族中的未知接口、尚未支持的新 Patch 版本以及
非 WASI Import，都会在加载状态机接口前被拒绝。v1.0 前会以可配置 Capability
策略替换这一固定基线，实现按 Task 授权和权限授予。

## 有限默认值

| 限制 | v0.1 默认值 |
| --- | --- |
| CPU 模式 | Epoch 中断 |
| Epoch 预算 | 100 Tick，进程 Tick 间隔 10 ms，并受墙钟时间上限约束 |
| 每次 Guest 调用的墙钟时间 | 1 秒 |
| 确定性 Fuel 模式 | 可配置的有限单位 |
| 每个 Store 的线性内存 | 64 MiB |
| Payload 数据 | 1 MiB |
| Payload Content Type / Schema ID | 各 4 KiB |
| 每个 Hook 可发出的事件数 | 256 |
| Run-to-completion 微步数 | 1,024 |

选择 Fuel 模式时会构建启用 Fuel 的 Wasmtime Engine；默认 Engine 使用 Epoch
中断。限制耗尽会报告为结构化 Runtime Fault，暂存的 State、Context 和 Event
会被丢弃。

第一份热路径测量基线和回归策略记录在
[`docs/benchmarks/v0.1-baseline.md`](docs/benchmarks/v0.1-baseline.md)。

## 兼容性与路线图

v0.x Host IR 和 WIT 可能发生不兼容变更。在 v1.0 前，请使用同一 Shiroha
Release/Revision 构建 Host 与 Component；项目不承诺自动进行 ABI 或 Snapshot
迁移。

从已经完成的 v0.1.0 本地库到生产可用 v1.0.0 的能力门禁路线记录在
[`ROADMAP.md`](ROADMAP.md)。下一版本 v0.2.0 将交付首个本地 `shirohad`
可执行程序、最小 REST 控制循环和 `sctl`。后续 v1 前门禁将依次加入 HSM 语义、
`redb` 持久化、框架级安全与 WASI Capability、无状态 Node、分布式调度与聚合、
真实 Adapter/扩展、OpenTelemetry、按角色打包以及兼容性加固。

多 Controller 共识和故障转移明确推迟到生产可用 v1.0 发布之后。

## Dev Setup

```bash
# Environment(Nix)
apt install -y direnv
echo 'use flake' > .envrc
direnv allow

# Environment
apt install -y rustup cargo-binstall protobuf-compiler pre-commit just

# Dev Tools
just install-dev

# AI(Optional)
npm install -g @mindfoldhq/trellis@latest @colbymchenry/codegraph
trellis init -u <your-name>
codegraph install
codegraph init

# Build
just build

# Release Build
just release

# Check
just check

# Format
just fmt

# Test
just test

# Coverage
just coverage

# Update
just update
```
