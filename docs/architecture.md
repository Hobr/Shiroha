# Shiroha Architecture

> 架构总览

## Status

- 状态：Draft
- 目标版本：v0.1-v0.2
- 当前实现：仓库目前仍以 workspace 骨架为主，本文描述的是目标架构，而不是现有功能列表
- 阅读约定：`必须` / `不能` 表示目标实现应满足的约束；`建议` / `候选` / `评估` 表示仍可调整

## 文档索引

- [执行模型](execution-model.md)
- [运行时与制品](runtime-and-artifacts.md)
- [接口设计](interfaces.md)

`architecture.md` 只描述系统总览、组件关系和角色形态。执行语义、故障恢复、Wasm 制品和 WIT 兼容等细节已拆分到专题文档中。

## 设计原则

- 状态迁移与副作用分离: 状态机只负责确定性推进, 外部 I/O 统一建模为 `Activity`
- 可恢复、可重放优先: `Event History` 是恢复与审计的权威来源
- 默认 `at-least-once`: 允许重复投递, 依赖幂等 Activity 保证正确性
- 单实例串行推进: 同一 `Workflow Instance` 在任意时刻只能有一个有效推进者
- 单一主程序、多种角色: 统一由 `shirohad` 承载控制面与执行面
- 先收敛执行模型, 再扩展分布式能力: 单机闭环稳定前不提前做多主控
- 版本绑定优先: 实例创建后锁定 Wasm 制品版本, 运行中不热切换

## 核心组成

| 组件 | 说明 |
| --- | --- |
| `Controller` | 控制面组件, 负责状态推进、调度、恢复和元数据维护 |
| `Executor` | 执行面组件, 负责运行 `Activity` 并回传结果 |
| `Workflow Definition` | 版本化工作流定义, 描述状态机行为和可调用 Activity |
| `Workflow Instance` | 某个工作流定义的一次实际运行 |
| `Artifact` | 可分发、缓存和校验的 Wasm 制品 |
| `WIT Interface` | 宿主暴露给 Wasm 的能力边界定义 |

## 整体模型

```text
┌─ Wasm (用户逻辑) ──────────────────────────────────┐
│  Workflow Definition                                │
│  ├─ 状态声明与转换规则                               │
│  ├─ 决策函数                                         │
│  └─ Activity 声明与实现                              │
└─────────────────────────────────────────────────────┘
             ↕ WIT Interface
┌─ Host (基础设施) ───────────────────────────────────┐
│  Controller                                          │
│  ├─ 事件落盘 / 状态推进 / 恢复                       │
│  ├─ TaskQueue / Timer / ArtifactRegistry            │
│  └─ Admin API                                        │
│                                                      │
│  Executor                                            │
│  ├─ 加载 Wasm Artifact                               │
│  ├─ 注入 Host Capability                             │
│  └─ 执行 Activity 并回传结果                         │
└─────────────────────────────────────────────────────┘
```

Host 与 Wasm 的交互分两类:

1. `Decision`: 基于确定性上下文做状态推进, 输出 `Command`
2. `Activity`: 执行带副作用的逻辑, 结果回写为历史事件

详细执行语义见 [执行模型](execution-model.md)。

## 角色架构

### v0.1: 单进程 hybrid

```text
┌─ shirohad ────────────────────────────────────────┐
│  Controller                                        │
│  ├─ StateMachineEngine / Recovery / Timer          │
│  ├─ TaskQueue (SQLite)                          │
│  └─ Admin API                                      │
│                                                    │
│  Executor                                          │
│  ├─ 加载 Wasm Artifact                             │
│  ├─ 注入 Host Capability                           │
│  └─ 返回 ActivityResult                            │
│                                                    │
│  Shared                                            │
│  ├─ SQLite                                         │
│  ├─ Event History / Snapshot                       │
│  └─ ArtifactStore                                  │
└───────────────────────────────────────────────────┘
```

### v0.2: 进程拆分

```text
┌─ Controller ──────────────────────┐
│  持久化 TaskQueue + Lease 管理     │
│  Node RPC Server                   │
└────────────────▲──────────────────┘
                 │ poll / ack / heartbeat / cancel
┌────────────────┴──────────────────┐
│ Executor                           │
│ Node RPC Client                    │
└───────────────────────────────────┘
```

能力声明、节点匹配、权限裁剪和接口边界的详细规则分别见 [运行时与制品](runtime-and-artifacts.md) 与 [接口设计](interfaces.md)。
