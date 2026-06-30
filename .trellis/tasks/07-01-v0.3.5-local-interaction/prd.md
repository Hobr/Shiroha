# v0.3.5: 本地交互增强 (REPL + Unix socket)

> 父任务：`06-29-shiroha-framework`  
> 版本规划：`.trellis/docs/version-roadmap.md`

## Goal

在 v0.3.0 基础上，增强 shirohad 的交互能力：添加 REPL 手动触发事件、支持多 component 加载、通过 Unix socket 暴露控制接口，使 sctl 能真正操作运行中的 shirohad。建立本地控制工具模式，为 v0.4.0 gRPC 远程控制打下基础。

**核心验证点**：sctl 可通过 Unix socket 连接本地 shirohad，查询 task 列表、发送事件、查看状态，REPL 模式下可交互式验证状态机行为。

## Background

当前状态（v0.3.0）：
- ✅ shirohad 守护进程可加载单个 WASM component
- ✅ 自动创建 task（ID 固定 "default"）
- ✅ 日志输出可观测
- ❌ 无法手动触发事件（状态机加载后无法推进）
- ❌ sctl 仅占位，无实际功能
- ❌ 无法加载多个 component

问题：
- 无法交互式验证状态机逻辑（需要手动触发迁移）
- 无法演示控制工具操作守护进程
- 无法测试多 task 并发场景

## Requirements

### R1: shirohad REPL 模式

**R1.1 CLI 参数新增**：
- `--repl` — 启用交互式 REPL 模式（可选，默认关闭）
- 非 REPL 模式：守护进程持续运行（v0.3.0 行为）
- REPL 模式：启动后进入交互式命令行

**R1.2 REPL 命令**：
- `status` — 显示所有 task 的当前状态
  - 输出：task ID / 当前状态 / component 路径
- `list-tasks` — 列出所有 task
  - 输出：task ID 列表
- `send-event <task-id> <event-name>` — 向指定 task 发送事件
  - 触发状态迁移
  - 输出：迁移结果（成功/失败 / 新状态）
- `help` — 显示可用命令列表
- `quit` / `exit` — 退出 REPL，停止守护进程

**R1.3 REPL 实现**：
- 使用 `rustyline` 或简单 `std::io::stdin()`
- 命令解析：简单 split + match
- 错误处理：无效命令输出错误提示，不退出 REPL
- 历史记录：可选（rustyline 自带）

**R1.4 REPL 与守护进程共存**：
- REPL 输入在单独 tokio task 中处理
- 不阻塞主守护进程循环
- 事件通过 channel 发送到 TaskManager

### R2: 多 Component 加载

**R2.1 CLI 参数修改**：
- `--component <path>` 可重复指定多次
  - 例：`shirohad --component a.wasm --component b.wasm`
- 每个 component 自动创建一个 task
- Task ID 生成规则：`<component-filename-without-ext>-<uuid-short>`
  - 例：`sm-example-a1b2`, `counter-c3d4`

**R2.2 Component 加载顺序**：
- 按 CLI 参数顺序依次加载
- 某个 component 加载失败：
  - 输出错误日志
  - 跳过该 component
  - 继续加载后续 component
- 全部加载失败：守护进程退出（exit code 1）

**R2.3 Task 管理**：
- TaskManager 管理多个 task
- 每个 task 独立运行（不相互影响）
- REPL / Unix socket 命令按 task ID 操作

### R3: Unix Socket 控制接口

**R3.1 Socket 文件路径**：
- 默认：`/tmp/shirohad.sock`（Linux/macOS）
- CLI 参数：`--socket <path>` 自定义路径
- 启动时创建 socket 文件
- 退出时删除 socket 文件

**R3.2 通信协议**：
- **传输层**：Unix domain socket（tokio `UnixListener`）
- **序列化**：JSON over newline-delimited text
- **请求格式**：
  ```json
  {
    "command": "list-tasks" | "send-event" | "task-status",
    "params": { ... }
  }
  ```
- **响应格式**：
  ```json
  {
    "status": "ok" | "error",
    "data": { ... } | null,
    "error": "error message" | null
  }
  ```

**R3.3 支持的命令**：
- `list-tasks` — 返回所有 task ID 列表
  - Response: `{"status": "ok", "data": {"tasks": ["id1", "id2"]}}`
- `send-event` — 发送事件到指定 task
  - Request: `{"command": "send-event", "params": {"task_id": "...", "event": "..."}}`
  - Response: `{"status": "ok", "data": {"new_state": "..."}}`
- `task-status` — 查询 task 当前状态
  - Request: `{"command": "task-status", "params": {"task_id": "..."}}`
  - Response: `{"status": "ok", "data": {"task_id": "...", "current_state": "...", "component": "..."}}`

**R3.4 错误处理**：
- 未知命令：`{"status": "error", "error": "Unknown command: ..."}`
- Task 不存在：`{"status": "error", "error": "Task not found: ..."}`
- 事件无效：`{"status": "error", "error": "Invalid event for current state"}`

**R3.5 并发处理**：
- 每个客户端连接在独立 tokio task 中处理
- 支持多个 sctl 实例同时连接
- TaskManager 通过 Arc + Mutex/RwLock 保护（或 message passing）

### R4: sctl 实际功能实现

**R4.1 连接参数**：
- `--socket <path>` — 指定 Unix socket 路径（默认 `/tmp/shirohad.sock`）
- 自动检测 socket 文件是否存在
- 连接失败：输出 "Cannot connect to shirohad (is it running?)"

**R4.2 命令实现**：
- `sctl list-tasks [--socket <path>]`
  - 发送 `list-tasks` 命令
  - 输出：表格或列表形式
- `sctl send-event <task-id> <event-name> [--socket <path>]`
  - 发送 `send-event` 命令
  - 输出：新状态或错误信息
- `sctl task-status <task-id> [--socket <path>]`
  - 发送 `task-status` 命令
  - 输出：task 详细信息（ID / 状态 / component）

**R4.3 剩余占位命令**：
- `create-task` / `stop-task` 继续输出 "This command will be implemented in v0.4.0"
- v0.3.5 不支持动态创建/停止 task（仅启动时加载）

**R4.4 依赖新增**：
- `sctl/Cargo.toml` 添加：
  - `tokio` (net feature for UnixStream)
  - `serde` + `serde_json`

### R5: 示例与文档

**R5.1 README 更新**：
- Quick Start 章节更新：
  - 添加 REPL 模式示例
  - 添加 sctl 操作示例
  - 添加多 component 加载示例

**R5.2 运行示例**：
```bash
# 1. 启动 shirohad（REPL 模式）
shirohad --component sm-example.wasm --repl

# 在 REPL 中：
> status
task: sm-example-a1b2, state: Idle

> send-event sm-example-a1b2 start
Transition: Idle -> Processing

> status
task: sm-example-a1b2, state: Processing

> quit

# 2. 启动 shirohad（守护进程模式）
shirohad --component sm-example.wasm --component counter.wasm

# 在另一个终端：
sctl list-tasks
# 输出：sm-example-a1b2, counter-c3d4

sctl task-status sm-example-a1b2
# 输出：Task: sm-example-a1b2, State: Idle, Component: sm-example.wasm

sctl send-event sm-example-a1b2 start
# 输出：Transition successful, new state: Processing
```

## Acceptance Criteria

### 必须完成

- [ ] `shirohad --repl` 启动交互式 REPL
- [ ] REPL 支持 4 个命令：status / list-tasks / send-event / quit
- [ ] `shirohad --component a.wasm --component b.wasm` 加载多个 component
- [ ] 每个 component 自动创建独立 task（不同 task ID）
- [ ] Unix socket 在 `/tmp/shirohad.sock` 创建
- [ ] Unix socket 支持 3 个命令：list-tasks / send-event / task-status
- [ ] `sctl list-tasks` 连接 shirohad 并返回 task 列表
- [ ] `sctl send-event <task-id> <event>` 触发状态迁移
- [ ] `sctl task-status <task-id>` 查询 task 状态
- [ ] 多个 sctl 实例可同时连接 shirohad
- [ ] README Quick Start 更新（REPL + sctl 示例）
- [ ] 手动验证通过（REPL 交互 + sctl 操作）

### 可选推迟

- [ ] REPL 历史记录（rustyline）— 可用 stdin 简化实现
- [ ] `create-task` / `stop-task` 实现 — v0.4.0
- [ ] Task 持久化 — v0.6.0
- [ ] 认证/授权 — v0.6.5

## Out of Scope

- **gRPC** — v0.4.0 实现（Unix socket → gRPC 迁移）
- **远程连接** — v0.4.0（Unix socket 仅本地）
- **动态创建 task** — v0.4.0（`create-task` 命令）
- **Task 停止/删除** — v0.4.0（`stop-task` 命令）
- **认证** — v0.6.5
- **持久化** — v0.6.0

## Technical Notes

### Task ID 生成

```rust
use uuid::Uuid;

fn generate_task_id(component_path: &Path) -> String {
    let name = component_path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("task");
    let uuid = Uuid::new_v4().to_string();
    format!("{}-{}", name, &uuid[..8])
}
```

### Unix Socket 协议示例

```rust
// shirohad/src/control.rs
#[derive(Deserialize)]
struct Request {
    command: String,
    params: serde_json::Value,
}

#[derive(Serialize)]
struct Response {
    status: String, // "ok" | "error"
    data: Option<serde_json::Value>,
    error: Option<String>,
}

async fn handle_connection(
    stream: UnixStream,
    manager: Arc<Mutex<TaskManager>>,
) -> Result<()> {
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();
    
    while let Some(line) = lines.next_line().await? {
        let req: Request = serde_json::from_str(&line)?;
        let resp = match req.command.as_str() {
            "list-tasks" => handle_list_tasks(&manager).await,
            "send-event" => handle_send_event(&manager, req.params).await,
            "task-status" => handle_task_status(&manager, req.params).await,
            _ => Response {
                status: "error".into(),
                data: None,
                error: Some(format!("Unknown command: {}", req.command)),
            },
        };
        
        let resp_line = serde_json::to_string(&resp)?;
        // write resp_line + "\n" to stream
    }
    Ok(())
}
```

### REPL 实现

```rust
// shirohad/src/repl.rs
async fn repl_loop(manager: Arc<Mutex<TaskManager>>) -> Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    
    println!("Shiroha REPL. Type 'help' for commands.");
    loop {
        print!("> ");
        std::io::stdout().flush()?;
        
        if let Some(line) = reader.next_line().await? {
            let parts: Vec<&str> = line.trim().split_whitespace().collect();
            match parts.first() {
                Some(&"status") => handle_status(&manager).await,
                Some(&"list-tasks") => handle_list_tasks(&manager).await,
                Some(&"send-event") if parts.len() == 3 => {
                    handle_send_event(&manager, parts[1], parts[2]).await
                }
                Some(&"quit") | Some(&"exit") => break,
                Some(&"help") => print_help(),
                _ => println!("Unknown command. Type 'help' for commands."),
            }
        }
    }
    Ok(())
}
```

## Dependencies

- **Blocks**: v0.4.0 gRPC 控制面（需要 Unix socket 控制接口经验）
- **Blocked by**: v0.3.0 shirohad 基础（已完成）

## Risks

- **Medium**: TaskManager API 可能需扩展（当前设计为单 task）
  - 需添加：`list_tasks()`, `get_task()`, `send_event(task_id, event)`
  - 风险：可能需重构 TaskManager 内部结构
- **Low**: Unix socket 协议设计（JSON 标准格式）
- **Low**: 多客户端并发（tokio UnixListener 标准用法）
- **Medium**: REPL 与守护进程共存（异步 stdin 读取可能复杂）
  - 缓解：使用 tokio::spawn + channel 隔离

## Success Metrics

- REPL 模式下可手动触发状态迁移并观察结果
- sctl 可操作运行中的 shirohad（list / send-event / status）
- 多 component 加载正常（2+ task 并发运行）
- Unix socket 支持多客户端同时连接
- 为 v0.4.0 gRPC 迁移打下基础（控制接口设计验证）

## Notes

这是一个**中等复杂度任务**：
- 需要设计 Unix socket 协议（JSON 格式）
- 需要扩展 TaskManager API（多 task 管理）
- 需要实现 REPL 循环（异步 stdin）

建议添加 `design.md`：
- Unix socket 协议详细规范
- TaskManager API 扩展设计
- REPL 与守护进程共存架构
- 并发控制策略（Arc + Mutex vs channel）

实现策略：
1. 先扩展 TaskManager API（支持多 task）
2. 实现 Unix socket server（最小协议）
3. 实现 sctl 客户端（3 个命令）
4. 实现 REPL 模式
5. 集成测试 + 手动验证
