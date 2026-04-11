# 项目概述

术语约定见 [名词表](./00-glossary.md)。
开发协作约定见 [开发规范](./14-development-guidelines.md)。

> WebAssembly 可扩展的状态机编排框架，支持将计算任务分发到本地或多节点执行并聚合结果。
>
> 文档定位：本文只提供目标架构总览。术语以 [名词表](./00-glossary.md) 为准，执行语义以 [执行语义](./11-execution-semantics.md) 为准，升级迁移以 [升级与迁移](./13-upgrades-and-migrations.md) 为准，阶段计划以 [Roadmap](./99-roadmap.md) 为准。

## 核心理念

用户在 WASM 模块内定义状态机及其 Action/Callback，宿主加载 WASM 解析定义、生成部署快照、驱动状态流转，并将任务按声明的策略分发到本地或远程节点执行，收集结果后按指定策略聚合，推动状态机向前。

## 设计原则

- **KISS** — 每个模块职责单一，模块间通过 trait 解耦
- **Effect 驱动** — 状态机引擎纯逻辑，不执行副作用，只产出意图（Effect），由上层解释执行
- **Host/Guest 分离** — WIT 契约定义双方边界，Host 通过能力注入为 Guest 提供 IO 等系统能力
- **能力即权限** — WASM 模块声明所需能力，Controller 在部署时生成授权结果，执行节点仅按该授权结果运行
- **单一二进制** — 通过配置决定运行模式，而非拆分为多个可执行文件
- **部署不可变** — 每次部署固化为 `deployment_id`，实例始终绑定创建时的部署快照

## 总体数据流

```
用户通过 SDK 编写 WASM 模块
    ├── 定义状态机（状态、转移、事件）
    ├── 声明依赖的 Host 能力（HTTP、KV、Log…）
    ├── 声明分发策略（本地 / 远程 N 节点）
    └── 实现 Action/Callback 逻辑

        ↓ 部署

Shiroha 实例 (Controller 模式)
    ├── 加载 WASM → 提取状态机定义 + 能力清单
    ├── 权限校验 → 生成 deployment manifest / 拒绝未授权能力
    ├── 创建状态机实例 → 绑定 deployment_id 并持久化初始状态
    └── 驱动状态流转
         ├── 产出 Effect::Execute → 分发层生成 task
         │    ├── 本地执行 → 直接调用 WASM 引擎
         │    └── 远程执行 → 通过传输层发送给 Node
         ├── 产出 Effect::Persist → 持久化层处理
         └── 产出 Effect::Complete → 标记终态

Shiroha 实例 (Node 模式)
    ├── 无状态，等待任务
    ├── 接收任务 → 按 deployment_id / wasm_hash 加载模块（本地缓存 / 向 Controller 拉取）
    ├── 校验 deployment manifest → 注入已授权的 Host 能力
    ├── 执行 Action → 返回结果
    └── 不参与状态管理
```
