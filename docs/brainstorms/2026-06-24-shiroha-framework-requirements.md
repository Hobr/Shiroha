---
date: 2026-06-24
topic: shiroha-framework
---

# Shiroha 框架需求

## Summary

Shiroha 是一个轻量、解耦的 Rust 状态机工作流编排框架,三层结构:状态机核心(可插拔的定义 adapter 与 action 执行类型)、可选的分布式分发器(把 action 派发到无状态 WASM 节点)、server 形态的控制器(任务管理 + 可选模块)。MVP 落地单机 + WASM adapter,状态机结构定义写在 WASM 组件内,其余能力预留接口。

## Problem Frame

传统工作流编排框架(Temporal、Airflow 等)功能强大但体量重、模块耦合深,小团队与轻量场景难以驾驭。Shiroha 的定位是用分层与可插拔把"功能强大"与"不复杂"拆开:核心保持小而通用,分布式与高级控制能力作为可选层叠加,用户按需启用而非一次面对全部复杂度。分布式分发是核心卖点,但默认单机即可跑通。

## Key Decisions

**轻量 + 功能强大靠分层化解。** 核心保持小而通用,强大功能(分布式、数据管理、观测、复杂聚合)通过可选模块与插件叠加,默认部署只跑核心。这是贯穿三层的设计哲学。

**分布式是核心卖点但可选。** 单机即可跑通完整闭环,分布式分发器是叠加层,接口预留但 MVP 不实现。节点无业务状态,只跑 action;状态机驱动(决策)在主控本地执行。

**权限下放给 Web 层。** 控制器只保留最小身份接口(身份 + 粗粒度放行),具体用户/角色/策略由 Web 层管理,框架不背认证鉴权的复杂度。

**adapter 只管状态机定义解析,action 执行类型是独立维度。** adapter(WASM Component / 文本)决定状态机结构怎么读;action/callback 的执行类型(wasm func / 框架插件)决定动作怎么跑。两者正交,不混为一谈。

**WASM 插件是开放扩展点。** 插件用 WASM 实现,初期只留注册/发现接口不提供官方插件。文本 adapter 定义的状态机可直接引用插件提供的能力(http 等),用户无需自己写代码。

## Actors

- A1. **主控(Master)** — `shirohad` 进程。持有状态机实例状态,驱动状态转移,聚合分发结果,暴露控制器接口。MVP 即主控单机运行。
- A2. **节点(Worker)** — `shirohad` worker 模式,无业务状态。内嵌 wasmtime,接收主控派发的 action 执行请求,回报结果。(预留,MVP 不实现)
- A3. **控制端客户端** — CLI(`sctl`)是首个客户端,通过控制器接口操作主控。未来 Web/GUI 共用同一接口。
- A4. **FSM 模块作者** — 提供 WASM 组件(声明状态机结构 + action 实现)的用户。MVP 唯一的"开发侧"角色。

## Requirements

**状态机核心(第一层)**

- R1. 框架定义统一的状态机模型(状态、转移规则、action/callback 引用),独立于任何 adapter 与执行类型。
- R2. 框架提供 adapter 抽象,用于从不同来源读出状态机结构。MVP 实现 WASM Component adapter;文本 adapter(TOML/YAML/JSON)预留接口不实现。
- R3. WASM adapter 从组件内读取状态机结构定义(运行时解析组件导出的描述符,而非在 WASM 内运行状态机本身)。
- R4. 状态机定义里每个 action/callback 声明自己的执行类型。MVP 支持 `wasm func`;框架插件作为执行类型预留接口不实现。
- R5. 框架定义插件注册/发现契约,插件用 WASM 实现,可被文本 adapter 定义的状态机直接引用。MVP 只留接口,不提供官方插件。
- R6. 状态机驱动(给定当前状态 + 输入,决定下一状态)在主控本地执行;主控内嵌 wasmtime。

**分布式分发(第二层,预留)**

- R7. 分发器是 Engine 与执行点之间的中间层,接口预留,MVP 不实现。单机 MVP 走本地执行路径。
- R8. 节点为无业务状态执行器,内嵌 wasmtime,按需从主控拉取 WASM 组件字节(主控为字节权威源)。接口预留。
- R9. 主控内置节点注册表(注册 + 心跳 + 健康标记),分发时据此选节点。接口预留。
- R10. 框架提供最小默认聚合(首个成功结果);复杂聚合策略走插件。接口预留。
- R11. 组件存储初期为框架内置,后续可外接 registry。存储层通过 trait 抽象。

**控制器(第三层)**

- R12. 控制器是独立进程,暴露接口让 GUI/CLI/Web 作为客户端连接(server 形态,非可嵌入库)。
- R13. 控制器核心提供任务管理:创建/查询/暂停/恢复/取消任务。
- R14. 控制器保留最小身份接口,接受身份 + 操作请求并做粗粒度放行;具体用户/角色/策略由 Web 层管理。
- R15. 数据管理与 OpenTelemetry 为可选模块,运行期通过配置/启动参数加载,不强制启用。
- R16. CLI(`sctl`)是首个客户端;Web/GUI 共用同一接口,后续接入。

**跨层与持久化**

- R17. MVP 在内存中运行,不落盘;持久化通过 trait 抽象留接口,后续接不同数据库后端。
- R18. 主控是状态的唯一权威源;节点不持有 Job 状态。

## Scope Boundaries

**Deferred for later(MVP 之后)**

- 文本 adapter(TOML/YAML/JSON)实现 —— 接口已留,adapter 机制就绪后再加
- 框架官方插件(http 等)实现 —— 插件契约已留,初期不提供
- 分布式分发(节点注册、远程派发、聚合)实现 —— 接口已留,单机 MVP 先跑通
- 持久化后端(数据库)实现 —— trait 已留,内存先行
- Web/GUI 客户端 —— CLI 先行,接口共用
- 节点面与控制面的具体协议细节

**Outside this product's identity**

- 框架不内置业务重试/补偿策略 —— 由用户在 FSM 定义中声明,引擎调度
- 框架不背认证鉴权体系 —— 权限下放给 Web 层
- 框架不做 HA/多主控 —— 单主控为 MVP 形态,HA 列入未来方向

## Dependencies / Assumptions

- 技术栈已定:Rust(edition 2024)、wasmtime(Component Model / WASI p3)、tonic(gRPC)、tokio。依赖已在 `Cargo.toml` 锁定。
- 仓库现状为空 workspace(`members = []`),无既有 crate 代码,本次为从零规划。
- git 历史中存在一整套前版架构文档(已在 "doc: remake" 提交删除),作为走过的路径可借鉴但不作基础。
- 假设:WASM Component Model 适合作为状态机定义 + action 实现的统一载体(前版已验证此方向,本项目沿用)。

## Outstanding Questions

**Resolve Before Planning**

- 无。核心边界与 MVP 范围已在对话中确认。

**Deferred to Planning**

- crate 划分与命名(前版有 `shiroha-core`/`shiroha-wasm`/`shiroha-engine` 等划分,是否沿用)
- 控制器接口的具体协议(gRPC / 其他)与 proto 定义
- adapter 与插件的注册机制具体形态(编译期 feature vs 运行期配置,已知倾向运行期)
- WASM 组件描述符的 WIT 接口形状
- 状态机模型的字段级细节(状态/转移/action 引用各自的具体结构)
