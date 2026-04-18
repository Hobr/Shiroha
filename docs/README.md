# Shiroha 设计文档

> 状态: Draft · 版本: 0.3.0 · 最后更新: 2026-04-18

Shiroha 是一个基于 Rust 的状态机编排框架。状态机定义与 Action/Callback 承载在 WASM 模块中，由宿主通过 Wasmtime 执行；重运算可按用户声明的策略分发到单个或多个节点 (本地/远程) 执行并聚合结果。

## 目录

1. [设计原则](01-principles.md)
2. [分层架构](02-architecture.md)
3. [Workspace 与 Crate 划分](03-workspace.md)
4. [关键抽象](04-abstractions.md)
5. [语义模型](05-semantics.md)
6. [WIT 接口设计](06-wit.md)
7. [数据流](07-dataflow.md)
8. [gRPC 服务划分](08-grpc.md)
9. [契约与 ABI](09-contracts.md)
10. [可观测性](10-observability.md)
11. [安全基线](11-security.md)
12. [短期实现与预留](12-roadmap.md)
