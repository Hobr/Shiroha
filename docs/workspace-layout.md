# Shiroha Workspace Layout

> 当前工作区结构、crate 边界与职责约定

## 当前状态

workspace 结构已经拆分完成，但各 app 和 crate 目前仍以模板代码为主。下面描述的是**目标职责边界**，用于指导后续实现落点。

## 顶层结构

| 路径 | 目标职责 |
| --- | --- |
| `app/shirohad` | 守护进程入口；承载 `controller`、`executor` 或 `hybrid` 角色 |
| `app/sctl` | 命令行入口；面向开发、调试和运维操作 |
| `crate/shiroha-core` | 核心领域模型、稳定类型、标识符、状态与事件定义 |
| `crate/shiroha-engine` | 状态机推进、重放、恢复与命令生成 |
| `crate/shiroha-runtime` | Wasm/WIT 运行时、宿主能力注入、执行上下文 |
| `crate/shiroha-sdk` | 用户工作流与 Activity 的声明方式、构建辅助与导出约束 |
| `crate/shiroha-store-sqlite` | SQLite 持久化实现，服务 v0.1 单机闭环 |
| `crate/shiroha-testkit` | 测试 harness、mock host capabilities、重放/恢复测试辅助 |

## 依赖方向

建议保持以下依赖方向：

- `app/*` 依赖底层 crate，不承载核心语义
- `shiroha-core` 尽量保持最少依赖，作为共享基础
- `shiroha-engine` 依赖 `shiroha-core`
- `shiroha-runtime` 依赖 `shiroha-core`，必要时与 `shiroha-engine` 协作
- `shiroha-sdk` 面向用户开发体验，不反向依赖 app
- `shiroha-store-sqlite` 作为基础设施实现，避免反向污染核心领域模型
- `shiroha-testkit` 可以依赖其他 crate，但业务 crate 不应依赖 `shiroha-testkit`

## 落点建议

在新增代码前，先判断它属于哪一层：

- 领域语义、状态、事件、稳定标识：放 `shiroha-core`
- 决策推进、重放、恢复算法：放 `shiroha-engine`
- Wasm 装载、WIT 边界、能力注入：放 `shiroha-runtime`
- 用户编写工作流的 API 与宏：放 `shiroha-sdk`
- SQLite schema、repository、事务边界：放 `shiroha-store-sqlite`
- 测试支持代码：放 `shiroha-testkit`
- 进程启动、配置解析、命令行入口：放 `app/*`

## 不建议的做法

- 新增 `common`、`utils`、`misc` 这类边界模糊的 crate
- 把运行时语义直接堆到 `app/shirohad`
- 让存储实现定义核心领域类型
- 让测试工具 crate 反向进入生产依赖链

如果某个能力暂时不好归类，优先回到设计文档中澄清边界，再落代码。
