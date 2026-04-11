# 名词表

> 本表用于统一 Shiroha 文档中的专用词汇。若其他文档与本表冲突，以本表为准。

## 使用规则

- 中文正文首次出现某个关键术语时，建议写成“中文名（英文名）”。
- `module`、`deployment`、`instance` 是三个不同层级的对象，不得混用。
- `task`、`attempt` 是调度层对象，不等同于状态机状态或业务事件。
- `dispatch` 指“决定发到哪里执行”，`aggregation` 指“决定多个结果如何合并”。
- 当前阶段只支持宿主内建的 dispatch / aggregation 策略；Guest 自定义策略属于长期规划。

## 系统角色

| 术语 | 定义 | 不要混用 |
|------|------|----------|
| Shiroha | 整个系统或项目名称 | 不等同于某个单独二进制 |
| Host | 承载 WASM 的宿主运行环境，负责能力注入与执行控制 | 不等同于 Controller |
| Guest | 运行在 Host 内部的用户 WASM 代码 | 不等同于模块作者或节点 |
| Controller | 负责部署、实例生命周期、task 调度、结果聚合的控制侧角色 | 不等同于 Node |
| Node | 负责接收 task 并执行 Action 的无状态执行侧角色 | 不负责状态机生命周期 |
| Standalone | Controller 与 Node 合体的单进程运行模式 | 不是新的角色，只是运行模式 |
| `shirohad` | Shiroha 的统一宿主进程入口 | 不等同于整个系统概念 |
| `sctl` | 面向运维/开发者的管理 CLI | 不负责业务执行 |

## 接口与开发侧

| 术语 | 定义 | 不要混用 |
|------|------|----------|
| WIT | Host / Guest 之间的接口契约描述语言 | 不等同于 Rust API |
| interface | WIT 中的一组函数边界定义，如 `http`、`kv`、`log` | 不等同于 world |
| world | WIT 中组合 imports / exports 的顶层组合单元 | 不等同于单个 interface |
| `definition` | Guest 导出的定义读取接口，用于返回状态机定义与能力声明 | 不负责实际执行 Action |
| `action` | Guest 导出的执行入口，用于执行 Action / Callback | 不等同于某个具体 Action 名称 |
| Guest SDK | 面向 Guest 开发者的运行时库，封装 WIT 绑定与能力 API | 不等同于过程宏 |
| `sdk` | 编译到 `wasm32-wasip2` 的 Guest 运行时 crate | 不等同于 `sdk-macros` |
| `sdk-macros` | 编译到 native 的 `proc_macro` crate，用于减少样板代码 | 不进入 Guest 运行时 |

## 工件与标识

| 术语 | 定义 | 不要混用 |
|------|------|----------|
| WASM 模块（module） | 原始的 Guest 二进制工件 | 不等同于 deployment |
| `wasm_hash` | 某个 WASM 模块内容的哈希标识 | 不是部署标识 |
| 部署（deployment） | 一次不可变部署快照，包含模块、授权结果和执行契约版本 | 不等同于实例 |
| `deployment_id` | 对外暴露的部署稳定标识 | 不要只用 `wasm_hash` 代替 |
| deployment manifest | 节点执行所需的部署描述，至少包含 `deployment_id`、`wasm_hash`、能力授权结果和契约版本 | 不等同于模块本体 |
| release alias | 指向“当前默认 deployment”的可变别名或路由指针 | 不等同于 deployment 本身 |
| Module Registry | Controller 维护的模块索引与拉取来源 | 不等同于 deployment registry |
| deployment registry | Controller 维护的部署快照索引 | 不等同于模块 blob 存储 |

## 状态机与执行模型

| 术语 | 定义 | 不要混用 |
|------|------|----------|
| 状态机（state machine / machine） | 由 Guest 定义、由宿主驱动流转的业务状态模型 | 不等同于某个实例 |
| Action | 一次可执行的业务逻辑单元，可由本地或远程节点运行 | 不等同于 Effect |
| Callback | 由状态推进或结果回收触发的回调执行入口 | 不等同于外部 RPC callback |
| Effect | `machine` 产出的执行意图，如 `Execute`、`Persist`、`Complete` | 不直接执行副作用 |
| 状态机实例（instance） | 某个状态机定义在某次部署下生成的运行中实体 | 不等同于 deployment |
| instance migration | 将一个现有 instance 从旧 `deployment_id` 显式迁移到新 `deployment_id` 的过程 | 不等同于重试 task |
| 事件日志（event log） | 记录实例推进过程中的关键事件与执行痕迹 | 不等同于 task result 集合 |

## 调度与聚合

| 术语 | 定义 | 不要混用 |
|------|------|----------|
| dispatch | 决定某个 Action 应在本地还是远程、以及发给哪些节点执行 | 不负责合并结果 |
| dispatch policy | 状态机声明的分发策略，如 `Local`、`RemoteAny`、`RemoteAll(n)` | 当前仅支持内建策略 |
| aggregation | 将多个执行结果合并为一个调度结论的过程 | 不决定执行位置 |
| aggregation policy | 状态机声明的聚合策略，如 `First`、`All`、`Majority` | 当前仅支持内建策略 |
| task | 调度层中的一次执行单元，绑定某个实例、部署与 Action 输入 | 不等同于业务事件 |
| `task_id` | task 的稳定标识，在重试期间保持不变 | 不等同于 `attempt` 标识 |
| attempt | 某个 task 的一次实际执行尝试 | 不等同于新的 task |
| task migration | 将尚未完成的 task 从旧 `deployment_id` 迁移到新 `deployment_id` 的过程 | 不等同于运行中的代码热替换 |
| Pure Action | 由 Guest 显式声明为可复制执行，且经 Host 校验只依赖确定性输入与允许能力子集的 Action | 不等同于“运行很快” |
| Effectful Action | 显式声明为 Effectful，或无法被 Host 验证为 Pure 的 Action；通常依赖外部副作用、时间、随机数或外部系统状态 | 默认不适合 `RemoteAll` / `Majority` |

## 能力与权限

| 术语 | 定义 | 不要混用 |
|------|------|----------|
| capability | Guest 请求 Host 提供的一项能力，如 `http`、`kv`、`log` | 不等同于某个具体配置项 |
| capability policy | Controller 用于决定哪些能力被授权的策略 | 不由 Node 重新解释 |
| capability materialization | 将 manifest 中的授权结果转换为节点执行期可用句柄或短期凭证的过程 | 不等同于 capability policy |
| runtime handle | Node 在某次 task / attempt 执行中实际拿到的能力句柄、短期 token 或连接上下文 | 不等同于 deployment manifest |
| 必需能力 | 缺失即拒绝部署的能力 | 不可降级为运行时报错 |
| 可选能力 | 未授权时可被替换为不可用桩的能力 | 不表示永远可安全忽略 |

## 运行时模块

| 术语 | 定义 | 不要混用 |
|------|------|----------|
| `model` | 纯数据模型 crate，定义部署、实例、task、结果等核心类型 | 不承载执行逻辑 |
| `engine` | WASM 运行时封装，负责模块加载、链接、能力注入与执行 | 不负责状态推进 |
| `machine` | 状态机纯逻辑引擎，只产出 Effect | 不直接做 IO |
| `dispatch` | 调度与聚合模块，负责 task 规划、执行路径选择、重试和结果合并 | 不负责持久化实现细节 |
| `transport` | Controller / Node 间的控制面和任务面通信抽象 | 不等同于业务协议 |
| `storage` | 实例、部署、task、日志等数据的持久化抽象 | 不等同于 Guest `kv` capability |
| 状态快照（snapshot） | 某个 instance 在某时刻的持久化状态镜像，需带 schema version 与来源 deployment 信息 | 不等同于 event log |

## 通信与持久化

| 术语 | 定义 | 不要混用 |
|------|------|----------|
| 控制面（control plane） | 面向部署、实例查询、节点注册、模块拉取的接口面 | 不等同于 task 执行链路 |
| 任务面（task plane） | 面向 task 下发、结果回传、取消、失败上报的接口面 | 不等同于管理接口 |
| 节点心跳（heartbeat） | Node 定期上报自身存活与健康状态的机制 | 不等同于 task 回报 |
| 租约（lease） | Controller 判断某个运行中 attempt 是否失联的时间约束 | 不等同于业务超时 |
| 超时（timeout / deadline） | 单个 task 或 attempt 的执行时限 | 不等同于租约 |

## 推荐用语

- 说“模块”时，优先指原始 WASM 二进制。
- 说“部署”时，优先指带授权与契约版本的不可变快照。
- 说“实例”时，优先指某次部署下实际运行的状态机实体。
- 说“执行”时，优先落到 `task` / `attempt` 层，而不是笼统说“跑一下”。
- 说“能力”时，优先指抽象 capability，而不是某个具体服务地址或凭证。
