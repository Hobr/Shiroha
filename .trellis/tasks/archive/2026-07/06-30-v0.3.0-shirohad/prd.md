# v0.3.0: shirohad 单机守护进程 + sctl 占位

> 父任务：`06-29-shiroha-framework`
> 版本规划：`.trellis/docs/version-roadmap.md`

## Goal

交付第一个可执行的 Shiroha 系统：`shirohad` 守护进程能加载 WASM component 并本地运行状态机 task，`sctl` 为未来控制工具占位。建立可演示、可验证的基准，为后续分布式架构打下基础。

**核心验证点**：shirohad 作为独立进程加载 WASM component，TaskManager 管理 task 生命周期，tracing 日志输出可观测。

## Background

当前状态：
- ✅ v0.2.5 完成：WasmAdapter + WasmActionInvoker + 集成测试验证
- ✅ 库代码可用：`shiroha-engine` / `shiroha-wasm` / `shiroha-ir`
- ❌ 无可执行文件：只能通过 `cargo test` 验证，无法独立运行

问题：
- 无法演示完整流程（"给用户看怎么用"）
- 无法进行端到端手动测试
- 无法为分布式架构提供基准二进制

## Requirements

### R1: shirohad 基础守护进程

**R1.1 CLI 参数**：
- `--component <path>` — 指定 WASM component 路径（必需）
- `--log-level <level>` — 日志级别（默认 `info`，支持 `debug` / `warn` / `error`）
- `--log-format <format>` — 日志格式（默认 `pretty`，支持 `json`）
- `--version` — 输出版本号
- `--help` — 输出帮助信息

**R1.2 WASM Component 加载**：
- 启动时加载单个 WASM component
- 使用 `WasmAdapter::load()` 解析为 `StateMachineDef` IR
- 加载失败打印错误并退出（exit code 1）
- 加载成功输出日志（component path / 状态数 / 迁移数）

**R1.3 Task 自动创建**：
- 从加载的状态机定义自动创建一个 task 实例
- Task ID 为 `default`（固定，单 task 场景）
- 使用 `TaskManager::create()`
- 初始状态输出到日志

**R1.4 守护进程运行**：
- 启动 tokio 异步运行时
- 主循环保持运行（不退出）
- 捕获 SIGTERM / SIGINT 优雅退出
  - 收到信号时停止 task
  - 输出 "Shutting down..." 日志
  - 退出进程（exit code 0）

**R1.5 日志输出**：
- 使用 `tracing` + `tracing-subscriber`
- 日志包含：
  - Component 加载（路径 / 状态数 / 迁移数）
  - Task 创建（task ID / 初始状态）
  - 状态迁移（from → to / event / timestamp）
  - Action 执行（action name / 成功/失败）
  - Host import 调用（`host.log` 内容）
- Pretty format 带颜色（终端输出）
- JSON format 单行输出（便于日志收集）

**R1.6 依赖管理**：
- `bin/shirohad/Cargo.toml` 依赖：
  - `shiroha-engine`
  - `shiroha-wasm`
  - `shiroha-ir`
  - `clap` (derive feature)
  - `tokio` (full)
  - `tracing` + `tracing-subscriber`
  - `anyhow`

### R2: sctl 占位工具

**R2.1 基础 CLI**：
- `--help` — 输出帮助信息
- `--version` — 输出版本号
- 无其他功能（所有命令输出 "Not implemented yet"）

**R2.2 命令结构预留**：
- 定义（但不实现）命令骨架：
  ```
  sctl create-task --component <path>
  sctl list-tasks
  sctl send-event <task-id> <event-name>
  sctl task-status <task-id>
  sctl stop-task <task-id>
  ```
- 每个命令返回 "This command will be implemented in v0.3.5"

**R2.3 依赖管理**：
- `bin/sctl/Cargo.toml` 依赖：
  - `clap` (derive feature)
  - `anyhow`

### R3: 示例演示

**R3.1 README 更新**：
- 添加 "Quick Start" 章节：
  ```bash
  # 编译
  cargo build --release

  # 编译 WASM component 示例
  cargo build --target wasm32-wasip2 -p shiroha-sm-example

  # 运行 shirohad
  ./target/release/shirohad --component ./target/wasm32-wasip2/debug/shiroha_sm_example.wasm
  ```
- 预期输出示例（日志截图或文本）

**R3.2 运行验证**：
- 手动运行 shirohad 加载 `sm-example.wasm`
- 观察日志输出：
  - Component 加载成功
  - Task 创建成功（初始状态 Idle）
  - 守护进程保持运行
  - SIGTERM 优雅退出

## Acceptance Criteria

### 必须完成

- [ ] `bin/shirohad/src/main.rs` 实现完整（非 "Hello, world!"）
- [ ] `shirohad --component <path>` 能加载 WASM component 并创建 task
- [ ] 日志输出包含：component 加载 / task 创建 / 初始状态
- [ ] 守护进程保持运行（不自动退出）
- [ ] SIGTERM / SIGINT 优雅退出（输出 "Shutting down..."）
- [ ] `sctl --help` / `sctl --version` 可用
- [ ] sctl 命令结构预留（5 个命令骨架）
- [ ] README Quick Start 章节完成
- [ ] 手动运行验证通过（加载 sm-example.wasm 成功）

### 可选推迟

- [ ] 事件注入机制（v0.3.5 实现 REPL）
- [ ] 多 component 加载（v0.3.5）
- [ ] sctl 实际功能（v0.3.5 Unix socket，v0.4.0 gRPC）

## Out of Scope

- **REPL 交互** — v0.3.5 实现（手动输入事件触发迁移）
- **sctl 实际功能** — v0.3.5 Unix socket，v0.4.0 gRPC
- **多 task 管理** — v0.3.0 仅支持单 task（ID 固定为 "default"）
- **持久化** — v0.6.0 实现
- **gRPC** — v0.4.0 实现
- **分布式** — v0.5.0 实现

## Technical Notes

### shirohad 架构

```rust
// bin/shirohad/src/main.rs
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. 解析 CLI 参数
    let args = Cli::parse();

    // 2. 初始化 tracing
    tracing_subscriber::fmt()
        .with_max_level(args.log_level)
        .init();

    // 3. 加载 WASM component
    let adapter = WasmAdapter::new()?;
    let def = adapter.load(&args.component).await?;
    tracing::info!("Loaded component: {} states, {} transitions",
        def.states.len(), def.transitions.len());

    // 4. 创建 task
    let manager = TaskManager::new();
    let task = manager.create("default", def).await?;
    tracing::info!("Created task: id={}, initial_state={}",
        task.id, task.current_state);

    // 5. 守护进程主循环
    let (tx, mut rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tx.send(()).ok();
    });

    rx.await.ok();
    tracing::info!("Shutting down...");

    Ok(())
}
```

### sctl 命令结构

```rust
// bin/sctl/src/main.rs
#[derive(Parser)]
#[command(name = "sctl", version, about = "Shiroha control tool")]
enum Cli {
    CreateTask {
        #[arg(long)]
        component: PathBuf,
    },
    ListTasks,
    SendEvent {
        task_id: String,
        event_name: String,
    },
    TaskStatus {
        task_id: String,
    },
    StopTask {
        task_id: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli {
        _ => eprintln!("This command will be implemented in v0.3.5"),
    }
    Ok(())
}
```

### 日志示例

```
2026-06-30T15:30:00.123Z  INFO shirohad: Starting Shiroha daemon
2026-06-30T15:30:00.456Z  INFO shirohad: Loading component: ./target/wasm32-wasip2/debug/shiroha_sm_example.wasm
2026-06-30T15:30:00.789Z  INFO shirohad: Component loaded: 3 states, 2 transitions, 2 events
2026-06-30T15:30:00.890Z  INFO shirohad: Created task: id=default, initial_state=Idle
2026-06-30T15:30:00.891Z  INFO shirohad: Daemon running, press Ctrl-C to stop
^C
2026-06-30T15:30:05.123Z  INFO shirohad: Shutting down...
```

## Dependencies

- **Blocks**: v0.3.5 本地交互（需要 shirohad 守护进程）
- **Blocked by**: v0.2.5 集成测试（已完成）

## Risks

- **Low**: CLI 参数解析简单（clap derive）
- **Low**: Tracing 配置标准（参考现有项目）
- **Medium**: TaskManager API 可能需微调（当前设计为测试用）
  - 可能需添加 `create_from_def()` 方法
  - Task ID 生成策略（固定 "default" vs 自动生成）

## Success Metrics

- 可以运行 `shirohad --component sm-example.wasm` 并看到日志
- 守护进程保持运行（不自动退出）
- SIGTERM 优雅退出（无 panic）
- README Quick Start 跟着操作可复现
- 为 v0.3.5 REPL / v0.4.0 gRPC 打下基础

## Notes

这是一个**轻量级任务**：
- 主要是组装现有库代码（engine/wasm/ir）
- CLI 参数解析 + tracing 配置是标准模式
- PRD-only 足够，无需 design.md / implement.md

实现策略：
1. 先实现 shirohad 基础版本（加载 + 创建 + 日志）
2. 手动测试验证
3. 实现 sctl 占位
4. 更新 README
5. 提交
