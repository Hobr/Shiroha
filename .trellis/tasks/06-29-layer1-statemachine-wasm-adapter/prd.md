# 第一层状态机核心与 WASM Component Model adapter

> 父任务：`06-29-shiroha-framework`

## Goal

交付第一层 MVP：一个高性能、功能强大的状态机核心，外加 WASM Component Model IR adapter。运行时读取 WASM 内声明的状态机结构构建内部 IR（而非把状态机引擎放进 WASM 运行）；action/callback 可选用 WASM 实现；文件 adapter（JSON/TOML）为后期功能占位（仅预留接口/不实现）。

## Confirmed Facts（来自仓库 inspection）

- 状态机核心需跑在 `shirohad` 守护进程内（tokio 异步运行时）。
- WASM 运行时已选 wasmtime 46 + wasmtime-wasi(p3)，组件目标 `wasm32-wasip2`。
- 状态机/action 插件编译目标为 wasm32-wasip2。
- 错误体系：thiserror（库错误）+ anyhow（应用层）。
- 性能取向：release profile 已配置 LTO/strip/opt3/codegen1/panic=abort。

## Requirements

- R1 状态机核心（层级 HSM）：嵌套状态、entry/exit action、迁移、事件、guard、history（shallow/deep）；MVP 不含正交/并发区域，但 IR 预留正交扩展位。
- R2 内部 IR：adapter 产出的统一中间表示，WASM adapter 与未来文件 adapter 共用；IR 需能表达嵌套状态树 + history。
- R3 WASM Component Model adapter：组件实现类型化细粒度 WIT 接口（states / transitions / actions / initial / events 等多个导出），运行时分片查询组装内部 IR；action/callback 为组件内单独导出函数，按名引用。WIT 接口本身是第一类契约产物。
- R4 action/callback 执行：区分两类——(a) 同步副作用 action（entry/exit/transition，fire-and-forget，快）；(b) 每状态至多一个 async do-activity（可 await、exit 时可取消、天然可分发，是第二层的分发单元）。支持 WASM 类型 action；框架插件 action（http/bash…）走 plugin 机制（MVP 至少一种）。迁移走 RTC（run-to-completion）。
- R4.1 plugin 扩展点系统：plugin 是通用框架扩展点系统，一个插件可注册一项或多项能力面——`ActionFunc`（MVP 实现 http func）、`Middleware`、`AggregationStrategy`（第二层）、`Transport`（分布式协议 rpc/p2p/消息服务，第二层）、`Adapter`（扩展状态机定义来源）。MVP 实现 `PluginRegistry` + `Plugin` trait + `ActionFunc`（http func），其余能力面仅留 trait + registry 存取留口。
- R5 task 实例化：从定义创建一个可执行 task 实例（actor 风格：mailbox + 可寻址）并驱动事件。`TaskManager`（engine 内）持有 TaskHandle map，是 task 生命周期唯一控制入口。
- R6 capability/授权：MVP 仅在 task 创建边界定义 `Authorizer`/capability trait 留口（默认 no-op 实现），不实现权限模型。
- R7 持久化：MVP 不做（纯内存运行），但 task 状态设计为可序列化，未来可插拔持久化 plugin 做快照。
- R8 文件 adapter：仅接口占位，不实现。
- R9 控制面边界契约：gRPC（tonic）控制面 service（`ShirohaControl` 供 sctl/Web/GUI 消费 + `NodeExecutor` 供 controller 分发 do-activity 到 node）定义 sctl↔shirohad 与 controller↔node 通信契约；两层安全（传输层 auth interceptor + capability authz）均 no-op 留缝；`TaskManager` 是控制面唯一操作 task 的入口。shirohad 通过 cargo feature 三形态编译：`full`（controller + 本地 node）、`controller`（仅控制端）、`node`（无状态执行端，注册到 controller）。proto/service impl 在 v0.4.0 实现，契约现在定型。

## Decisions（已定）

- D1 形式化 = 层级 HSM（嵌套 + entry/exit + history），task 实例 actor 风格；MVP 无正交/并发区域，IR 预留扩展位。
- D2 action 模型 = 同步副作用 action + 每状态至多一个 async do-activity（可 await/cancel，第二层分发单元）；迁移走 RTC。
- D3 WASM adapter 提取契约 = 声明式细粒度 WIT 接口（分片查询组装 IR）；WIT 接口即第一类契约。
- D4 持久化 = MVP 不做（纯内存），task 状态可序列化，未来 plugin 快照。
- D5 capability = MVP 仅在 task 创建边界留 `Authorizer` trait 接缝（默认 no-op），不写权限逻辑。host import 面即为未来 capability 面。

## Acceptance Criteria

- [ ] 一个 wasm32-wasip2 组件能被 adapter 读取为状态机 IR。
- [ ] 从该 IR 能实例化一个 task，注入事件后正确迁移状态并执行 WASM action/callback。
- [ ] guard 命中可阻止迁移；action 回调结果可影响后续迁移。
- [ ] 状态机核心单元测试 + WASM adapter 集成测试通过（`just test`）。
- [ ] capability hook 有显式留口（注释/类型标记），后续可接入。

## Out of Scope

- 文件 adapter（JSON/TOML）实现（仅接口占位）。
- 分布调度器（第二层）。
- 控制器与 GUI/CLI/Web（第三层；`sctl` 仅为占位）。
- 完整 capability 权限模型实现（仅留 trait 留口）。
- 持久化/崩溃恢复实现（仅保证状态可序列化）。

## Technical Notes

- 事件模型 / RTC / mailbox、IR schema、action 统一签名、WIT 接口草图、crate 布局见 `design.md`。
- 有序执行计划与验证命令见 `implement.md`。
