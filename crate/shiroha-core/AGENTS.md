<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-02 -->

# shiroha-core

## Purpose

框架的基础类型库。定义所有共享的数据结构和 trait 抽象，是所有其他 crate 的依赖根。零内部 crate 依赖。

## Key Files

| File | Description |
| ---- | ----------- |
| `src/flow.rs` | 状态机定义：FlowManifest、StateDef、TransitionDef、ActionDef、DispatchMode 等 |
| `src/job.rs` | Job 运行实例：JobState、Job、ExecutionStatus、ActionResult、AggregateDecision |
| `src/event.rs` | 事件溯源：EventRecord、EventKind（Created/Transition/Paused/Completed 等） |
| `src/storage.rs` | `Storage` trait + `MemoryStorage`（开发/测试用内存实现） |
| `src/transport.rs` | `Transport` trait + `InProcessTransport`（standalone 模式用） |
| `src/error.rs` | `ShirohaError` 统一错误枚举 + `Result<T>` 别名 |

## For AI Agents

### Working In This Directory

- 修改类型定义后需全量 `cargo check --workspace` — 所有 crate 都依赖此库
- `Storage` 和 `Transport` trait 使用 `impl Future` 返回位置语法（非 `async_trait`）
- 所有公开类型派生 `Serialize, Deserialize, Debug, Clone`
- 枚举使用 `#[serde(rename_all = "snake_case")]`

### Testing Requirements

- `cargo check -p shiroha-core`
- 类型变更后检查 serde 序列化兼容性（`EventKind` 使用 `tag = "type"` 内部标签）

### Common Patterns

- trait 方法签名：`fn method(&self, ...) -> impl Future<Output = Result<T>> + Send`
- MemoryStorage 使用 `Arc<RwLock<HashMap>>` 实现线程安全

<!-- MANUAL: -->
