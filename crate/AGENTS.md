<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-04-02 | Updated: 2026-04-06 -->

# crate

## Purpose

Workspace 库 crate 目录。按关注点分离为 7 个独立 crate，通过 trait 抽象解耦。

## Subdirectories

| Directory | Purpose |
| --------- | ------- |
| `shiroha-client/` | 面向 CLI / 交互端的客户端抽象，包装 proto client 并返回领域类型（见 `shiroha-client/AGENTS.md`） |
| `shiroha-core/` | 核心类型与 trait 定义（见 `shiroha-core/AGENTS.md`） |
| `shiroha-sdk/` | Rust guest 开发 SDK，封装 WIT world 生成宏和常用 helper（见 `shiroha-sdk/AGENTS.md`） |
| `shiroha-wit/` | canonical WIT 定义，供 SDK / 文档 / 宿主侧测试共享 |
| `shiroha-engine/` | 状态机引擎、Job 管理、定时器（见 `shiroha-engine/AGENTS.md`） |
| `shiroha-proto/` | gRPC protobuf 服务定义（见 `shiroha-proto/AGENTS.md`） |
| `shiroha-store-redb/` | Redb 嵌入式存储后端（latest flow、version history、wasm bytes）（见 `shiroha-store-redb/AGENTS.md`） |
| `shiroha-wasm/` | WASM 运行时集成（见 `shiroha-wasm/AGENTS.md`） |

## For AI Agents

### Working In This Directory

- `shiroha-core` 是所有其他 crate 的基础依赖，修改其类型/trait 后需全量检查
- 新增存储/传输后端作为独立 crate 放在此目录下
- crate 命名规范：`shiroha-{module}` 或 `shiroha-{layer}-{backend}`

### Dependency Order

```
shiroha-core (零内部依赖)
  ↑
shiroha-engine, shiroha-wasm, shiroha-store-redb (依赖 core)

shiroha-proto (独立，不依赖其他内部 crate)
  ↑
shiroha-client (依赖 proto)

shiroha-wit (独立，提供 canonical WIT 定义)
  ↑
shiroha-sdk (独立，面向 guest 侧，依赖 wit-bindgen 和 shiroha-wit)
```

<!-- MANUAL: -->
