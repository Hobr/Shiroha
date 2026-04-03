<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-03 -->

# docs

## Purpose

架构设计文档，描述框架的设计决策、组件职责和实施路线。作为开发参考，不包含 API 文档（由 `cargo doc` 生成）。

## Key Files

| File | Description |
| ---- | ----------- |
| `architecture.md` | 总览：系统架构图、节点模式、子文档索引 |
| `core-concepts.md` | 核心概念：Flow、Job（生命周期/并发控制）、Execution、子流程 |
| `wasm-design.md` | WASM 层：manifest 接口、分发模式、Plugin 体系、权限系统 |
| `scheduling.md` | 调度与定时器：dispatch mode、策略、故障处理、背压 |
| `event-sourcing.md` | 事件溯源：记录格式、审计追踪、故障恢复 |
| `security.md` | 安全：节点认证（Join Token / mTLS）、WASM 沙箱 |
| `backends.md` | 可插拔后端：Transport、Storage、Context 传递 |
| `operations.md` | 运维：节点管理、观测性、拓扑模式 |
| `roadmap.md` | 路线图：Flow 验证、四阶段实施计划 |

## For AI Agents

### Working In This Directory

- 文档只描述"做什么"和"为什么"，不包含代码
- 修改架构决策后同步更新相关文档
- `architecture.md` 是入口索引，保持子文档链接有效
- 设计文档需要区分“当前已实现”与“后续阶段目标”，避免把未来能力写成现状

<!-- MANUAL: -->
