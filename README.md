# Shiroha

> 由 WebAssembly 驱动的分布式状态机任务编排框架

目前处于早期设计与开发阶段。

## 简介

Shiroha 关注一件事: 用可恢复、可重放的状态机驱动任务编排，并把带副作用的执行单元安全地分发到其他节点上运行。

项目的核心思路是:

- 用状态机负责确定性的状态推进
- 用 `Activity` 承载非确定性逻辑和外部副作用
- 用 Wasm + WIT 作为用户逻辑与宿主能力之间的边界
- 用统一主程序 `shirohad` 承载主控、节点或混合部署角色

## 核心组成

| 组件 | 说明 |
| --- | --- |
| `shirohad` | 统一守护进程, 可运行在 `controller`、`executor` 或 `hybrid` 模式 |
| `sctl` | 命令行工具, 用于注册、启动、查询、取消和调试工作流 |
| `Controller` | 控制面, 负责状态推进、任务调度、恢复和元数据维护 |
| `Executor` | 执行面, 负责运行 Activity 并回传结果 |
| `Workflow Definition` | 用户定义的状态机与 Activity 声明, 编译为 Wasm 制品 |
| `Activity` | 带副作用的执行单元, 可本地执行也可远程分发执行 |

## 设计文档

- [架构总览](docs/architecture.md)
- [执行模型](docs/execution-model.md)
- [运行时与制品](docs/runtime-and-artifacts.md)
- [接口设计](docs/interfaces.md)

`README` 只保留项目入口信息，详细执行语义、故障恢复、版本生命周期和调度模型统一放在设计文档中。

## 路线图

### v0.1

- 单机闭环: 事件历史、状态快照、状态机执行引擎
- 确定性决策边界与 Activity 执行语义
- Wasmtime + 最小 WIT 接口
- 本地 `Admin API` 与 `sctl`

### v0.2

- 单主控多节点执行
- 持久化任务队列、租约、心跳与回执
- gRPC 节点通信
- Wasm 制品分发、节点能力匹配与本地编译缓存

### v0.3

- PostgreSQL 存储（v0.1–v0.2 使用 SQLite，不承诺数据自动迁移；生产使用建议从 v0.3 开始）
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
git clone https://github.com/Hobr/Shiroha.git
cd Shiroha

# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装工具链
cargo install just cargo-binstall

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
