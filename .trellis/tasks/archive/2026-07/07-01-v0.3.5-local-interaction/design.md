# Design: v0.3.5 本地交互增强 (REPL + Unix socket)

> 父任务：`06-29-shiroha-framework`
> PRD：`prd.md`

## 1. 概述

在 v0.3.0 单机守护进程基础上，增加四个能力：

1. **TaskHandle 状态查询** — engine 层最小改动，使 REPL / socket 能读取 task 当前状态
2. **Unix socket 控制接口** — shirohad 暴露 JSON-over-newline 协议，sctl 可连接操作
3. **REPL 交互模式** — shirohad `--repl` 进入命令行，手动触发事件
4. **多 Component 加载** — `--component` 可重复，每个 component 创建独立 task

### 代码库现状（v0.3.0）

| 组件 | 文件 | 现状 |
|------|------|------|
| `TaskManager` | `crates/engine/src/runtime.rs:440` | ✅ 已支持多 task（`Arc<RwLock<HashMap<TaskId, TaskHandle>>>`），已有 `create_task` / `get_task` / `list_tasks` |
| `TaskHandle` | `crates/engine/src/task.rs:18` | ❌ 仅有 `sender`，无法查询状态（Task 被 `run()` 消费） |
| `Task` | `crates/engine/src/runtime.rs:128` | `get_state()` 存在但只能在 Task 内部调用 |
| `shirohad` main | `bin/shirohad/src/main.rs` | 单 `--component`，固定 task ID "default"，仅 ctrl_c 等待 |
| `sctl` main | `bin/sctl/src/main.rs` | 全部占位，无 tokio / Unix socket |
| `shiroha-control` | `crates/control/src/lib.rs` | 空壳（默认 `cargo init` 模板） |

**关键发现**：PRD 中 R3 风险「TaskManager API 可能需扩展」已基本缓解 — `TaskManager` 原生支持多 task。真正的缺口是 `TaskHandle` 无法查询状态。

---

## 2. Engine 层改动：TaskHandle 状态查询

### 2.1 问题

`Task::run(self)` 消费 `Task` 并移入 `tokio::spawn`。`TaskHandle` 只持有 `mpsc::UnboundedSender<Event>`，无法读取 task 当前状态。REPL `status` 和 socket `task-status` 都依赖此能力。

### 2.2 方案：共享 `Arc<RwLock<TaskState>>`（用户确认方案 A）

在 `Task::new()` 中创建 `Arc<RwLock<TaskState>>`，Task 和 TaskHandle 各持一份克隆：

```
Task::new()
  │
  ├── Task { ..., state: Arc<RwLock<TaskState>> }
  │     ├── enter_state() → 更新 state (初始状态)
  │     └── execute_transition() → 更新 state (迁移后)
  │
  └── TaskHandle { ..., state: Arc<RwLock<TaskState>> }
        └── get_state() → 读 state（无需 channel 往返）
```

### 2.3 改动清单

**`crates/engine/src/task.rs`**:

```rust
pub struct TaskHandle {
    id: TaskId,
    sender: mpsc::UnboundedSender<Event>,
    state: Arc<RwLock<TaskState>>,          // 新增
    component_path: Option<PathBuf>,         // 新增：task 来源 component 路径
}

impl TaskHandle {
    pub fn new(
        id: TaskId,
        sender: mpsc::UnboundedSender<Event>,
        state: Arc<RwLock<TaskState>>,
        component_path: Option<PathBuf>,
    ) -> Self { ... }

    /// 读取 task 当前状态（无 async，锁内直接构造 TaskState clone）
    pub fn get_state(&self) -> TaskState {
        self.state.read().blocking_clone()   // 或 async 版本
    }

    /// 返回 task 来源 component 路径
    pub fn component_path(&self) -> Option<&Path> { ... }
}
```

> **注意**：`TaskState` 已 derive `Clone`，`RwLock` 读锁内可直接 clone 返回。

**`crates/engine/src/runtime.rs`**:

```rust
impl Task {
    pub fn new(
        id: TaskId,
        def: StateMachineDef,
        action_invoker: Arc<dyn ActionInvoker>,
        guard_evaluator: Arc<dyn GuardEvaluator>,
        component_path: Option<PathBuf>,       // 新增参数
    ) -> (Self, TaskHandle) {
        // ...
        let state = Arc::new(RwLock::new(TaskState {
            task_id: id.clone(),
            current_state: def.initial.clone(),
            active_do_activity: None,
        }));

        let task = Self { ..., state: state.clone() };
        let handle = TaskHandle::new(id, sender, state, component_path);
        (task, handle)
    }

    pub fn run(mut self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let initial_state = self.def.initial.clone();
            self.enter_state(&initial_state).await;
            self.update_shared_state().await;        // 新增：同步初始状态
            while let Some(event) = self.receiver.recv().await {
                self.process_event(event).await;
                self.update_shared_state().await;    // 新增：迁移后同步
            }
        })
    }

    async fn update_shared_state(&self) {
        let mut state = self.state.write().await;
        let config = self.config.read().await;
        let do_activity = self.do_activity_handle.read().await;
        state.current_state = config.current_state.clone();
        state.active_do_activity = if do_activity.is_some() {
            Some("active".to_string())
        } else {
            None
        };
    }
}
```

**`TaskManager::create_task`** 签名扩展：

```rust
pub async fn create_task(
    &self,
    id: TaskId,
    def: StateMachineDef,
    action_invoker: Arc<dyn ActionInvoker>,
    guard_evaluator: Arc<dyn GuardEvaluator>,
    component_path: Option<PathBuf>,       // 新增
) -> anyhow::Result<TaskHandle>
```

### 2.4 向后兼容

- `TaskManager` API 签名变化（新增 `component_path` 参数）。调用方仅 `bin/shirohad/src/main.rs` 和 `crates/engine/src/tests.rs`，同步更新即可。
- `TaskHandle::new()` 签名变化。调用方仅 `runtime.rs` 内部，同步更新。
- 不影响序列化格式（`TaskState` 结构不变）。

---

## 3. 控制协议：`shiroha-control` crate

### 3.1 职责

`crates/control` 作为共享协议层，存放 shirohad（server）和 sctl（client）共用的 Request / Response 类型。遵循 workspace 分层规则：integration 层，依赖 engine + ir。

### 3.2 类型定义

```rust
// crates/control/src/protocol.rs

use serde::{Deserialize, Serialize};

/// 客户端请求（JSON over newline-delimited Unix socket）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", content = "params")]
pub enum Request {
    /// 列出所有 task ID
    #[serde(rename = "list-tasks")]
    ListTasks,

    /// 向指定 task 发送事件
    #[serde(rename = "send-event")]
    SendEvent {
        task_id: String,
        event: String,
    },

    /// 查询 task 当前状态
    #[serde(rename = "task-status")]
    TaskStatus {
        task_id: String,
    },
}

/// 服务端响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub status: ResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ResponseStatus {
    Ok,
    Error,
}

impl Response {
    pub fn ok(data: serde_json::Value) -> Self {
        Self { status: ResponseStatus::Ok, data: Some(data), error: None }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self { status: ResponseStatus::Error, data: None, error: Some(msg.into()) }
    }
}
```

### 3.3 序列化示例

```json
// list-tasks 请求
{"command":"list-tasks"}

// send-event 请求
{"command":"send-event","params":{"task_id":"sm-a1b2","event":"start"}}

// task-status 请求
{"command":"task-status","params":{"task_id":"sm-a1b2"}}

// 成功响应
{"status":"ok","data":{"tasks":["sm-a1b2","counter-c3d4"]}}
{"status":"ok","data":{"task_id":"sm-a1b2","current_state":"Processing","component":"sm-example.wasm"}}
{"status":"ok","data":{"new_state":"Processing"}}

// 错误响应
{"status":"error","error":"Task not found: sm-a1b2"}
{"status":"error","error":"Unknown command: foo"}
```

### 3.4 crate 依赖

```toml
# crates/control/Cargo.toml
[dependencies]
shiroha-engine = { workspace = true }   # 引用 TaskState 等类型
serde = { workspace = true }
serde_json = { workspace = true }
```

> `shiroha-control` 依赖 `shiroha-engine`（integration → execution），符合 workspace 分层规则。

---

## 4. shirohad 架构改动

### 4.1 模块布局

```
bin/shirohad/src/
├── main.rs         # CLI 解析、Daemon 创建、shutdown 协调
├── daemon.rs       # Daemon 结构体：持有 TaskManager + 元数据，加载 component
├── control.rs      # Unix socket server：accept 连接、分发命令
└── repl.rs         # REPL 循环：async stdin、命令解析
```

### 4.2 CLI 参数变更

```rust
#[derive(Parser, Debug)]
struct Cli {
    /// Path to WASM component (repeatable)
    #[arg(long)]
    component: Vec<PathBuf>,

    /// Enable interactive REPL mode
    #[arg(long, default_value_t = false)]
    repl: bool,

    /// Unix socket path
    #[arg(long, default_value = "/tmp/shirohad.sock")]
    socket: PathBuf,

    /// Log level
    #[arg(long, default_value = "info")]
    log_level: Level,

    /// Log format
    #[arg(long, default_value = "pretty")]
    log_format: LogFormat,
}
```

变更点：
- `component: PathBuf` → `component: Vec<PathBuf>`（`--component` 可重复）
- 新增 `--repl`、`--socket`

### 4.3 Daemon 结构体

```rust
// bin/shirohad/src/daemon.rs

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use shiroha_engine::TaskManager;

pub struct Daemon {
    /// Task 管理器（内部 Arc，clone 低成本）
    task_manager: TaskManager,
    /// task ID → component 路径（元数据，控制层需要）
    component_paths: Arc<RwLock<HashMap<String, PathBuf>>>,
    /// Unix socket 文件路径
    socket_path: PathBuf,
    /// 全局取消令牌
    cancel_token: CancellationToken,
}
```

> **设计决策**：component 路径由 shirohad 跟踪（不入 engine crate）。engine 层关注运行时执行，component 路径是部署/加载元数据。`TaskHandle` 也携带 `component_path` 作为便捷字段，但权威来源是 `Daemon::component_paths`。

### 4.4 多 Component 加载流程

```rust
impl Daemon {
    /// 加载多个 component，返回成功创建的 task 数量
    pub async fn load_components(&self, paths: &[PathBuf]) -> Result<usize> {
        let engine = self.create_wasm_engine()?;
        let mut success_count = 0;

        for path in paths {
            match self.load_single_component(&engine, path).await {
                Ok(task_id) => {
                    info!("Task created: id={}, component={}", task_id, path.display());
                    success_count += 1;
                }
                Err(e) => {
                    error!("Failed to load component {}: {}", path.display(), e);
                    // 跳过，继续加载后续 component
                }
            }
        }

        if success_count == 0 {
            anyhow::bail!("All components failed to load");
        }

        Ok(success_count)
    }

    async fn load_single_component(&self, engine: &Engine, path: &Path) -> Result<String> {
        let task_id = generate_task_id(path);
        let adapter = WasmAdapter::from_file(engine.clone(), path)?;
        let def = adapter.load().await?;
        let action_invoker = Arc::new(WasmActionInvoker::from_file(engine.clone(), path)?);
        let guard_evaluator = Arc::new(NoopGuardEvaluator);

        let handle = self.task_manager
            .create_task(task_id.clone(), def, action_invoker, guard_evaluator, Some(path.to_path_buf()))
            .await?;

        // 记录 component 路径
        self.component_paths.write().await.insert(task_id.clone(), path.to_path_buf());

        Ok(task_id)
    }
}
```

### 4.5 Task ID 生成

```rust
fn generate_task_id(component_path: &Path) -> String {
    let name = component_path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("task");
    let uuid = uuid::Uuid::new_v4();
    format!("{}-{}", name, &uuid.to_string()[..8])
}
```

---

## 5. Unix Socket Server

### 5.1 模块：`bin/shirohad/src/control.rs`

```rust
pub async fn run_socket_server(
    task_manager: TaskManager,
    component_paths: Arc<RwLock<HashMap<String, PathBuf>>>,
    socket_path: PathBuf,
    cancel_token: CancellationToken,
) -> Result<()> {
    // 清理旧 socket 文件
    let _ = std::fs::remove_file(&socket_path);

    let listener = UnixListener::bind(&socket_path)
        .context(format!("Failed to bind socket: {}", socket_path.display()))?;

    info!("Unix socket listening: {}", socket_path.display());

    loop {
        tokio::select! {
            biased;
            () = cancel_token.cancelled() => {
                break;
            }
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _)) => {
                        let tm = task_manager.clone();
                        let cp = component_paths.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, tm, cp).await {
                                error!("Connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Accept error: {}", e);
                    }
                }
            }
        }
    }

    // 清理 socket 文件
    let _ = std::fs::remove_file(&socket_path);
    info!("Socket server stopped");
    Ok(())
}
```

### 5.2 连接处理

```rust
async fn handle_connection(
    stream: UnixStream,
    task_manager: TaskManager,
    component_paths: Arc<RwLock<HashMap<String, PathBuf>>>,
) -> Result<()> {
    let reader = BufReader::new(stream.clone());
    let mut writer = stream;
    let mut lines = reader.lines();

    while let Some(line) = lines.next_line().await? {
        let resp = match serde_json::from_str::<Request>(&line) {
            Ok(req) => dispatch(req, &task_manager, &component_paths).await,
            Err(e) => Response::error(format!("Invalid request: {}", e)),
        };
        let resp_line = serde_json::to_string(&resp)?;
        writer.write_all(resp_line.as_bytes()).await?;
        writer.write_all(b"\n").await?;
    }
    Ok(())
}
```

### 5.3 命令分发

```rust
async fn dispatch(
    req: Request,
    task_manager: &TaskManager,
    component_paths: &Arc<RwLock<HashMap<String, PathBuf>>>,
) -> Response {
    match req {
        Request::ListTasks => {
            let tasks = task_manager.list_tasks().await;
            Response::ok(serde_json::json!({"tasks": tasks}))
        }
        Request::SendEvent { task_id, event } => {
            match task_manager.get_task(&task_id).await {
                Some(handle) => match handle.send(Event::new(event, None)) {
                    Ok(()) => {
                        let state = handle.get_state();
                        Response::ok(serde_json::json!({"new_state": state.current_state}))
                    }
                    Err(e) => Response::error(format!("Failed to send event: {}", e)),
                }
                None => Response::error(format!("Task not found: {}", task_id)),
            }
        }
        Request::TaskStatus { task_id } => {
            match task_manager.get_task(&task_id).await {
                Some(handle) => {
                    let state = handle.get_state();
                    let component = component_paths.read().await
                        .get(&task_id)
                        .map(|p| p.display().to_string())
                        .unwrap_or_default();
                    Response::ok(serde_json::json!({
                        "task_id": state.task_id,
                        "current_state": state.current_state,
                        "component": component,
                    }))
                }
                None => Response::error(format!("Task not found: {}", task_id)),
            }
        }
    }
}
```

> **注意**：`send-event` 返回的 `new_state` 是发送后的当前状态。由于 RTC（run-to-completion）语义，事件处理是同步的 — 但 tokio channel 是异步的，状态可能在响应时尚未更新。这是 v0.3.5 的已知限制：返回的是「事件已投递」时的状态，而非「迁移完成后」的状态。v0.4.0 gRPC 可通过 request-response RPC 解决。文档中会标注此限制。

### 5.4 并发模型

- `TaskManager` 内部 `Arc<RwLock<HashMap>>`，天然支持多读者 / 单写者
- `TaskManager` 可直接 clone（内部 Arc），传入每个连接的 tokio task
- `component_paths` 同样 `Arc<RwLock<HashMap>>`
- 多个 sctl 实例可同时连接，每个连接独立 tokio task

---

## 6. REPL 模块

### 6.1 模块：`bin/shirohad/src/repl.rs`

使用 `tokio::io::stdin()` 异步读取，不引入 rustyline 依赖（PRD 标注为可选）。

```rust
pub async fn run_repl(
    task_manager: TaskManager,
    component_paths: Arc<RwLock<HashMap<String, PathBuf>>>,
    cancel_token: CancellationToken,
) -> Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();

    println!("Shiroha REPL. Type 'help' for commands.");

    loop {
        print!("> ");
        std::io::stdout().flush()?;

        tokio::select! {
            biased;
            () = cancel_token.cancelled() => break,

            line = reader.next_line() => {
                match line? {
                    Some(input) => {
                        if let Some(should_quit) = handle_repl_command(&input, &task_manager, &component_paths).await {
                            if should_quit {
                                cancel_token.cancel();
                                break;
                            }
                        }
                    }
                    None => break, // EOF
                }
            }
        }
    }
    Ok(())
}
```

### 6.2 REPL 命令

| 命令 | 说明 | 对应 socket 命令 |
|------|------|-----------------|
| `status` | 显示所有 task 状态 | — |
| `list-tasks` | 列出所有 task ID | `list-tasks` |
| `send-event <id> <event>` | 发送事件 | `send-event` |
| `help` | 显示帮助 | — |
| `quit` / `exit` | 退出（触发 cancel） | — |

```rust
async fn handle_repl_command(
    input: &str,
    task_manager: &TaskManager,
    component_paths: &Arc<RwLock<HashMap<String, PathBuf>>>,
) -> Option<bool> {  // 返回 Some(true) 表示应退出
    let parts: Vec<&str> = input.trim().split_whitespace().collect();
    match parts.first() {
        Some(&"status") => {
            let ids = task_manager.list_tasks().await;
            for id in ids {
                if let Some(handle) = task_manager.get_task(&id).await {
                    let state = handle.get_state();
                    println!("  task: {}, state: {}", state.task_id, state.current_state);
                }
            }
        }
        Some(&"list-tasks") => {
            let ids = task_manager.list_tasks().await;
            println!("{}", ids.join(", "));
        }
        Some(&"send-event") if parts.len() == 3 => {
            match task_manager.get_task(parts[1]).await {
                Some(handle) => match handle.send(Event::new(parts[2].to_string(), None)) {
                    Ok(()) => {
                        let state = handle.get_state();
                        println!("Transition: -> {}", state.current_state);
                    }
                    Err(e) => println!("Error: {}", e),
                }
                None => println!("Task not found: {}", parts[1]),
            }
        }
        Some(&"help") => print_help(),
        Some(&"quit") | Some(&"exit") => return Some(true),
        Some(cmd) => println!("Unknown command: {}. Type 'help' for commands.", cmd),
        None => {}
    }
    Some(false)
}
```

---

## 7. sctl 客户端重写

### 7.1 依赖变更

```toml
# bin/sctl/Cargo.toml
[dependencies]
shiroha-control = { workspace = true }   # 共享 Request/Response 类型
clap.workspace = true
anyhow.workspace = true
tokio.workspace = true                   # 新增
serde.workspace = true                   # 新增
serde_json.workspace = true              # 新增
```

### 7.2 CLI 结构

```rust
#[derive(Parser, Debug)]
#[command(name = "sctl", version, about = "Shiroha control tool")]
struct Cli {
    /// Unix socket path
    #[arg(long, default_value = "/tmp/shirohad.sock", global = true)]
    socket: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// List all running tasks
    ListTasks,
    /// Send an event to a task
    SendEvent { task_id: String, event_name: String },
    /// Get task status
    TaskStatus { task_id: String },
    /// (v0.4.0) Create a new task
    CreateTask { #[arg(long)] component: PathBuf },
    /// (v0.4.0) Stop a running task
    StopTask { task_id: String },
}
```

### 7.3 客户端连接

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::CreateTask { .. } | Commands::StopTask { .. } => {
            eprintln!("This command will be implemented in v0.4.0");
            return Ok(());
        }
        _ => {}
    }

    let mut stream = UnixStream::connect(&cli.socket)
        .with_context(|| format!(
            "Cannot connect to shirohad (is it running? socket: {})",
            cli.socket.display()
        ))?;

    let req = match &cli.command {
        Commands::ListTasks => Request::ListTasks,
        Commands::SendEvent { task_id, event_name } => Request::SendEvent {
            task_id: task_id.clone(), event: event_name.clone(),
        },
        Commands::TaskStatus { task_id } => Request::TaskStatus { task_id: task_id.clone() },
        _ => unreachable!(),
    };

    // 发送请求
    let req_line = serde_json::to_string(&req)?;
    stream.write_all(req_line.as_bytes()).await?;
    stream.write_all(b"\n").await?;

    // 读取响应
    let reader = BufReader::new(&mut stream);
    let mut lines = reader.lines();
    if let Some(resp_line) = lines.next_line().await? {
        let resp: Response = serde_json::from_str(&resp_line)?;
        match resp.status {
            ResponseStatus::Ok => print_ok_response(&cli.command, resp.data),
            ResponseStatus::Error => {
                eprintln!("Error: {}", resp.error.unwrap_or_default());
                std::process::exit(1);
            }
        }
    }
    Ok(())
}
```

### 7.4 输出格式

- `list-tasks`：逗号分隔 task ID 列表
- `send-event`：`Transition successful, new state: <state>`
- `task-status`：`Task: <id>, State: <state>, Component: <path>`

---

## 8. Shutdown 协调

### 8.1 CancellationToken 流

```
                 ┌──────────────┐
   ctrl_c ──────▶│              │
                 │  cancel()    │
   REPL quit ───▶│              │
                 └──────┬───────┘
                        │
           ┌────────────┼────────────┐
           ▼            ▼            ▼
     Socket Server   REPL Loop    main()
     (select!        (select!     (wait +
      cancelled)     cancelled)    cleanup)
```

### 8.2 main.rs 结构

```rust
#[tokio::main]
async fn main() -> Result<()> {
    let args = Cli::parse();
    init_tracing(&args)?;

    if args.component.is_empty() {
        anyhow::bail!("At least one --component required");
    }

    let cancel_token = CancellationToken::new();
    let daemon = Daemon::new(args.socket.clone(), cancel_token.clone());

    // 加载 component
    daemon.load_components(&args.component).await?;

    // 启动 Unix socket server
    let socket_handle = tokio::spawn(run_socket_server(
        daemon.task_manager.clone(),
        daemon.component_paths.clone(),
        daemon.socket_path.clone(),
        cancel_token.clone(),
    ));

    // ctrl_c → cancel
    let ctrl_token = cancel_token.clone();
    tokio::spawn(async move {
        if signal::ctrl_c().await.is_ok() {
            ctrl_token.cancel();
        }
    });

    // REPL 或守护进程模式
    if args.repl {
        run_repl(
            daemon.task_manager.clone(),
            daemon.component_paths.clone(),
            cancel_token.clone(),
        ).await?;
    } else {
        info!("Daemon running, press Ctrl-C to stop");
        cancel_token.cancelled().await;
    }

    // 等待 socket server 退出
    let _ = socket_handle.await;
    info!("Shutting down...");
    Ok(())
}
```

### 8.3 清理

- socket 文件：`run_socket_server` 在退出时 `remove_file`；启动时也先清理残留
- tokio task：socket server 通过 `cancel_token` 优雅停止；REPL 同理
- Task 运行时：当前不主动 abort task（task 生命周期随进程退出结束）

---

## 9. 依赖变更

### 9.1 workspace 根 `Cargo.toml`

```toml
[workspace.dependencies]
# 新增
uuid = { version = "1", features = ["v4"] }
```

> `tokio-util` 已存在（features = ["time"]），需追加 `"rt"` feature 以使用 `CancellationToken`：
> `tokio-util = { version = "0.7", features = ["time", "rt"] }`

### 9.2 各 crate 变更

| crate | 新增依赖 | 说明 |
|-------|---------|------|
| `shiroha-control` | `shiroha-engine`, `serde`, `serde_json` | 协议类型 |
| `shiroha-engine` | 无新增 | 仅内部改动 |
| `shirohad` | `shiroha-control`, `serde`, `serde_json`, `uuid`, `tokio-util` | socket/repl/多组件 |
| `sctl` | `shiroha-control`, `tokio`, `serde`, `serde_json` | Unix socket client |

### 9.3 engine tests 更新

`crates/engine/src/tests.rs` 中 `Task::new()` 和 `TaskManager::create_task()` 调用需更新签名（新增 `component_path` 参数，传 `None`）。

---

## 10. 兼容性与回滚

### 10.1 兼容性

- **CLI 向后兼容**：`--component` 从单值变 `Vec`，旧用法 `--component a.wasm` 仍有效
- **socket 文件**：默认 `/tmp/shirohad.sock`，与 sctl 默认值一致
- **协议**：JSON-over-newline，无版本号字段。v0.4.0 gRPC 替换时协议完全迁移，无需版本协商

### 10.2 已知限制（v0.3.5）

1. **send-event 响应状态**：事件投递后立即读取状态，可能不是迁移最终状态（RTC 处理是异步的）。这是 channel 模型的固有限制。
2. **无 task 停止**：task 生命周期随进程结束，无 `stop-task` 命令。
3. **无认证**：任何本地进程可连接 socket。
4. **socket 文件竞态**：如果 shirohad 崩溃未清理 socket，下次启动会先 `remove_file`。

### 10.3 回滚

所有改动在单一分支上。如果需要回滚：
1. `git revert` 整个 commit
2. engine API 签名变化会导致 tests 编译失败，但仅影响 `tests.rs` 和 `shirohad/main.rs`，回滚后自动恢复

---

## 11. 依赖关系图

```
bin/sctl ──────────┐
                   ▼
            crates/control (协议类型)
                   │
                   ▼
            crates/engine (TaskManager, TaskHandle, TaskState)
                   │
                   ▼
            crates/ir (StateMachineDef, State, Transition)

bin/shirohad ──▶ crates/control + crates/engine + crates/wasm
                          │              │
                          ▼              ▼
                     crates/ir     crates/engine → crates/ir
```

无循环依赖。`shiroha-control` 不依赖 `shiroha-wasm`（wasm 是 shirohad 的直接依赖，不经 control 层）。

---

## 12. 测试策略

### 12.1 单元测试

| 层级 | 测试点 | 位置 |
|------|--------|------|
| engine | `TaskHandle::get_state()` 返回正确状态 | `crates/engine/src/tests.rs` |
| engine | `TaskManager` 多 task 管理 | `crates/engine/src/tests.rs` |
| control | `Request` / `Response` 序列化/反序列化 | `crates/control/src/lib.rs` |
| shirohad | Task ID 生成（filename + uuid8） | `bin/shirohad/src/daemon.rs` |

### 12.2 集成测试

| 场景 | 验证方式 |
|------|---------|
| 多 component 加载 | 启动 shirohad `--component a --component b`，检查 task 数量 |
| Unix socket 通信 | 启动 shirohad，用 sctl 发送 3 个命令 |
| REPL 交互 | 手动验证（stdin 交互难以自动化） |
| 多客户端并发 | 2 个 sctl 同时连接 |

### 12.3 手动验证（Acceptance Criteria）

按 PRD R5.2 示例流程：
1. `shirohad --component sm-example.wasm --repl` → REPL 交互
2. `shirohad --component a.wasm --component b.wasm` + sctl 操作
