# Implement: v0.3.5 本地交互增强 (REPL + Unix socket)

> 依赖：`prd.md`（需求）、`design.md`（技术设计）
> 父任务：`06-29-shiroha-framework`

## 执行顺序

按 design.md 的分层依赖关系，从底层（engine）到上层（sctl/REPL）依次实现。

---

### Step 1: engine 层 — TaskHandle 状态查询

**目标**：使 `TaskHandle` 能查询 task 当前状态，携带 component 路径。

**改动文件**：
- `crates/engine/src/task.rs` — `TaskHandle` 增加 `state: Arc<RwLock<TaskState>>` 和 `component_path: Option<PathBuf>` 字段，增加 `get_state()` / `component_path()` 方法
- `crates/engine/src/runtime.rs` — `Task` 增加 `state` 字段；`Task::new()` 创建共享 state、接受 `component_path` 参数；`Task::run()` 在 `enter_state` 和 `process_event` 后调用 `update_shared_state()`；`TaskManager::create_task()` 签名增加 `component_path` 参数
- `crates/engine/src/tests.rs` — 更新所有 `Task::new()` / `create_task()` 调用，补 `None` 参数

**验证**：
```bash
cargo check -p shiroha-engine
cargo nextest run -p shiroha-engine
```

**Review gate**：engine crate 独立编译 + 测试通过。

**回滚点**：此步仅改 engine 内部，不影响其他 crate 编译（tests 同步更新）。

---

### Step 2: control 协议层 — `shiroha-control` crate

**目标**：定义共享的 `Request` / `Response` / `ResponseStatus` 类型。

**改动文件**：
- `crates/control/Cargo.toml` — 添加 `shiroha-engine`、`serde`、`serde_json` 依赖
- `crates/control/src/lib.rs` — 替换空壳内容，声明 `mod protocol` 并 `pub use protocol::*`
- `crates/control/src/protocol.rs`（新建）— `Request` enum（`#[serde(tag = "command", content = "params")]`）、`Response` struct、`ResponseStatus` enum、`Response::ok()` / `Response::error()` 构造方法

**验证**：
```bash
cargo check -p shiroha-control
cargo nextest run -p shiroha-control
```

**Review gate**：control crate 编译通过，序列化/反序列化单元测试通过。

---

### Step 3: workspace 依赖更新

**目标**：添加 `uuid`，追加 `tokio-util` 的 `rt` feature。

**改动文件**：
- `Cargo.toml`（workspace 根）— `[workspace.dependencies]` 添加 `uuid = { version = "1", features = ["v4"] }`；修改 `tokio-util` 行追加 `"rt"` feature

**验证**：
```bash
cargo check --workspace
```

---

### Step 4: shirohad — Daemon 结构体 + 多 component 加载

**目标**：重构 main.rs，提取 Daemon 结构体，支持多 component 加载和 task ID 生成。

**改动文件**：
- `bin/shirohad/Cargo.toml` — 添加 `shiroha-control`、`serde`、`serde_json`、`uuid`、`tokio-util` 依赖
- `bin/shirohad/src/daemon.rs`（新建）— `Daemon` 结构体（`task_manager`、`component_paths`、`socket_path`、`cancel_token`）、`load_components()` / `load_single_component()` / `generate_task_id()`
- `bin/shirohad/src/main.rs` — CLI 参数改为 `Vec<PathBuf>` component + `--repl` + `--socket`；创建 Daemon；加载多 component

**验证**：
```bash
cargo check -p shirohad
```

**Review gate**：shirohad 编译通过（此步不引入 socket/REPL，仅结构重构 + 多组件加载）。

**手动验证**：
```bash
cargo build --target wasm32-wasip2 -p shiroha-sm-example
./target/release/shirohad --component ./target/wasm32-wasip2/debug/shiroha_sm_example.wasm
# 检查日志显示 task 创建成功
```

---

### Step 5: shirohad — Unix socket server

**目标**：实现 Unix socket 控制接口，支持 3 个命令。

**改动文件**：
- `bin/shirohad/src/control.rs`（新建）— `run_socket_server()`、`handle_connection()`、`dispatch()` 函数
- `bin/shirohad/src/main.rs` — 在 daemon 初始化后 `tokio::spawn` socket server，传入 `cancel_token`

**验证**：
```bash
cargo check -p shirohad
```

**手动验证**（需 sctl 或手动 nc）：
```bash
# 启动 shirohad
./target/release/shirohad --component ./target/wasm32-wasip2/debug/shiroha_sm_example.wasm &
# 手动测试
echo '{"command":"list-tasks"}' | nc -U /tmp/shirohad.sock
```

**Review gate**：socket server 启动，能响应 list-tasks。

---

### Step 6: sctl 客户端重写

**目标**：sctl 通过 Unix socket 连接 shirohad，实现 3 个命令。

**改动文件**：
- `bin/sctl/Cargo.toml` — 添加 `shiroha-control`、`tokio`、`serde`、`serde_json` 依赖
- `bin/sctl/src/main.rs` — 改为 `#[tokio::main]` async；CLI 增加 `--socket` 全局参数；`ListTasks` / `SendEvent` / `TaskStatus` 实现 Unix socket 通信；`CreateTask` / `StopTask` 输出 v0.4.0 占位

**验证**：
```bash
cargo check -p sctl
```

**Review gate**：sctl 编译通过。

---

### Step 7: shirohad — REPL 模式

**目标**：实现 `--repl` 交互式命令行。

**改动文件**：
- `bin/shirohad/src/repl.rs`（新建）— `run_repl()`、`handle_repl_command()`、`print_help()`
- `bin/shirohad/src/main.rs` — `--repl` 时调用 `run_repl()`，否则等待 ctrl_c / cancel

**验证**：
```bash
cargo check -p shirohad
```

---

### Step 8: Shutdown 协调 + main.rs 整合

**目标**：统一 shutdown 流程（ctrl_c / REPL quit → cancel → socket 清理）。

**改动文件**：
- `bin/shirohad/src/main.rs` — ctrl_c spawn → cancel；REPL `quit` → cancel；socket server `select!` cancelled；退出时 `remove_file(socket)`

**验证**：
```bash
cargo check -p shirohad
```

---

### Step 9: 全量质量检查

**验证命令**：
```bash
just check          # cargo check --workspace
just test           # cargo nextest run --all-features --run-ignored all
just fmt            # cargo fmt + pre-commit
cargo clippy --workspace -- -D warnings
cargo deny check
```

**Review gate**：全部通过。

---

### Step 10: 集成手动验证

**验证脚本**（按 PRD R5.2）：

```bash
# 构建
cargo build --release
cargo build --target wasm32-wasip2 -p shiroha-sm-example

# 1. REPL 模式
./target/release/shirohad \
  --component ./target/wasm32-wasip2/debug/shiroha_sm_example.wasm \
  --repl
# REPL 中输入: status / list-tasks / send-event <id> start / status / quit

# 2. 守护进程 + sctl
./target/release/shirohad \
  --component ./target/wasm32-wasip2/debug/shiroha_sm_example.wasm \
  --socket /tmp/shirohad.sock &

./target/release/sctl list-tasks
./target/release/sctl task-status <task-id>
./target/release/sctl send-event <task-id> start

# 3. 多 component（如有第二个 component）
./target/release/shirohad --component a.wasm --component b.wasm

# 4. 多客户端并发
./target/release/sctl list-tasks & ./target/release/sctl list-tasks &
```

**验收标准**：对照 PRD Acceptance Criteria 全部勾选。

---

## 风险文件与回滚

| 步骤 | 风险文件 | 回滚方式 |
|------|---------|---------|
| Step 1 | `crates/engine/src/task.rs`, `runtime.rs` | git revert；仅影响 engine crate |
| Step 4-8 | `bin/shirohad/src/*` | git revert；恢复 v0.3.0 单文件 main.rs |
| Step 6 | `bin/sctl/src/main.rs` | git revert；恢复占位版本 |

## 跟进检查

- [ ] engine API 签名变更是否同步更新所有调用方（shirohad + tests）
- [ ] socket 文件清理在异常退出路径是否覆盖
- [ ] `tokio-util` feature 追加后不影响 engine 现有 `time` 用法
- [ ] `uuid` crate 是否加入 `deny.toml` 许可证白名单
