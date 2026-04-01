# Shiroha

> 由 WebAssembly 驱动的分布式状态机任务编排框架

目前处于早期设计与开发阶段。

当前仓库以架构设计文档和 Rust workspace 骨架为主；`app/shirohad`、`app/sctl` 与各 `crate/*` 目前仍是占位实现。本文档和 `docs/` 中描述的运行时、接口与分布式语义，当前应视为设计目标，而不是已经完成的功能清单。

## 简介

Shiroha 关注一件事：用可恢复、可重放的状态机驱动任务编排，并把带副作用的执行单元安全地分发到其他节点上运行。

项目的核心思路是：

- 用状态机负责确定性的状态推进
- 用 `Activity` 承载非确定性逻辑和外部副作用
- 用 Wasm + WIT 作为用户逻辑与宿主能力之间的边界
- 用统一主程序 `shirohad` 承载主控、节点或混合部署角色

## 当前状态

- 已完成：workspace 拆分、Rust 工具链固定、基础开发脚手架、设计文档整理
- 未完成：`Controller` / `Executor` 运行时、`Admin API`、`Node RPC`、Wasm 制品生命周期、持久化与调度闭环
- 当前 `app/` 和 `crate/` 下的源码仍以模板实现为主，适合继续做模块边界与接口收敛，不适合当作可运行产品使用

## 目标核心组成

| 组件 | 说明 |
| --- | --- |
| `shirohad` | 规划中的统一守护进程，可运行在 `controller`、`executor` 或 `hybrid` 模式 |
| `sctl` | 规划中的命令行工具，用于注册、启动、查询、取消和调试工作流 |
| `Controller` | 控制面，负责状态推进、任务调度、恢复和元数据维护 |
| `Executor` | 执行面，负责运行 `Activity` 并回传结果 |
| `Workflow Definition` | 用户定义的状态机与 Activity 声明，编译为 Wasm 制品 |
| `Activity` | 带副作用的执行单元，可本地执行也可远程分发执行 |

## 工作区布局

| 路径 | 角色 |
| --- | --- |
| `app/shirohad` | 守护进程入口 |
| `app/sctl` | CLI 入口 |
| `crate/shiroha-core` | 核心领域模型与基础类型 |
| `crate/shiroha-engine` | 状态机推进与重放执行引擎 |
| `crate/shiroha-runtime` | Wasm/WIT 运行时与宿主能力注入 |
| `crate/shiroha-sdk` | 用户侧工作流/Activity SDK |
| `crate/shiroha-store-sqlite` | v0.1 存储后端 |
| `crate/shiroha-testkit` | 测试工具与执行 harness |

更详细的职责边界见 [工作区布局](docs/workspace-layout.md)。

## 文档

### 设计文档

- [架构总览](docs/architecture.md)
- [执行模型](docs/execution-model.md)
- [运行时与制品](docs/runtime-and-artifacts.md)
- [接口设计](docs/interfaces.md)

### 开发文档

- [开发说明](docs/development.md)
- [工作区布局](docs/workspace-layout.md)

`README` 只保留项目入口、当前状态和导航信息；详细执行语义、故障恢复、版本生命周期和调度模型统一放在 `docs/` 中。

## 路线图

### v0.1

- 单机闭环：事件历史、状态快照、状态机执行引擎
- 确定性决策边界与 Activity 执行语义
- Wasmtime + 最小 WIT 接口
- 本地 `Admin API` 与 `sctl`

### v0.2

- 单主控多节点执行
- 持久化任务队列、租约、心跳与回执
- gRPC 节点通信
- Wasm 制品分发、节点能力匹配与本地编译缓存

### v0.3

- PostgreSQL 存储（v0.1-v0.2 使用 SQLite，不承诺数据自动迁移；生产使用建议从 v0.3 开始）
- tracing / metrics / 审计
- Signal / Query
- secret 管理、认证授权、Web 管理界面

### v0.4

- 嵌套状态机
- 更完整的 WIT Host 扩展
- 更完善的 SDK 与开发工具链
- 多租户能力

### v0.5

- 多主控 / Raft
- 插件系统
- 可选节点通信后端扩展

## 开发

```bash
# Rust环境
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo install just cargo-binstall
rustup target add wasm32-wasip2

# 构建
just build

# 开发
pip install pre-commit
just install-dev
just fmt
just doc

# 更新
just update

# 发布
just release
```
