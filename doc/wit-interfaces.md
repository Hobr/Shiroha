# WIT 接口设计 (shiroha-wit)

## 设计理念

WIT 是用户 FSM 模块与 Shiroha 之间的唯一契约面。

- 模块**导出** (exports) 自身的 FSM 描述、转移决策、Action 实现、可选自定义聚合
- 模块**导入** (imports) 主机受控能力(网络、KV、时钟、日志);任何 WASI 不直接提供或需要主机调控权限的能力都走这层

`shiroha-wit` 仅放置 `.wit` 文件与 wit-bindgen 生成的绑定。它本身不实现任何能力——主机能力的实现在 `shiroha-wasm`。

## World 划分

预期至少存在一个 world `shiroha:guest`,但用户的 WASM 组件最终可以编译为一份产物,被主控与节点分别按需调用其 export 子集:

- 主控加载组件后只调用与"驱动"相关的 export(描述符、转移决策、聚合)
- 节点加载组件后只调用与"Action 执行"相关的 export
- 同一组件在主从两端各跑一份;部署一次,执行点不同

MVP 暂保持单一 world;若将来节点端体积成为瓶颈,可拆为主控版 / 节点版两个独立 world。

## 导出 (Exports) 概念组

| 概念组 | 用途 | 由谁调用 |
| --- | --- | --- |
| 描述符 | 给主控读出 FSM 静态结构(状态、转移、Action、分发策略) | 主控 |
| 决策 | 给定当前状态与输入,给出下一状态 | 主控 |
| Action 实现 | 给定 ActionRef 名称与参数,执行并返回结果 | 节点(主控自托管 worker 时也调) |
| 聚合 (可选) | 用户自定义的聚合函数 | 主控 |

具体函数签名在 `.wit` 文件中确定;此处只锁定概念组的存在与调用方。

## 导入 (Imports) — 主机能力

Shiroha 对 WASM 提供的能力总集**保持小而稳定**。MVP 阶段最终能力清单:

- **log** — 层级化日志输出,落入主机的 tracing 体系
- **clock** — wall-clock / monotonic 时间
- **net.http** — 出站 HTTP,**仅 GET + POST**;代理与超时受主机配置控制(PUT / PATCH / DELETE 等暂不开放,有真实需求时再追加)
- **kv** — 小型键值存储,**作用域为 per-Job**;Job 终态后自动清理。Job 间共享数据走 FSM 转移输入,而非 KV
- **fs.readonly** — 只读访问主控配置的**白名单目录**(用于配置文件、静态资源);**不暴露任何写文件能力**
- **rand** — 仅 crypto-safe(CSPRNG);**不暴露 deterministic RNG**,避免被误用于业务逻辑

不开放的能力(默认拒绝):任意文件读写 / scratch dir / 子进程 / 原始 socket / 自定义 DNS / 自定义时区。新增能力需在本节显式记录其安全语义后方可加入。

## 能力授予粒度

每个 Flow 在 sctl 上传命令的 metadata 中声明它需要哪些能力(WASM component 内也可附带能力清单作为参考,但**以 sctl 入参为准**)。主控加载组件时按 Flow 记录中的声明授予;声明之外的能力调用一律 trap,避免静默扩权。

授予粒度是 per-Flow,同一 Flow 的所有 Job 共享同一组能力上限。

## 部署与版本

- 用户 FSM 模块通过控制面上传到主控
- 主控对组件字节按内容 hash 去重存储,主键为 `ComponentId`
- Flow(name + version + 能力声明)是主控层的人类语义包装,引用一个 ComponentId
- 节点端只看到 ComponentId,不感知 Flow / 版本
- 节点按需 pull:首次执行某 ComponentId 上的 Action 时,通过节点面 transport 向主控拉取字节(见 `worker.md`)

## 失败语义

WIT 调用层面区分两类失败:

- **协议错误** — component trap、调用越界、能力拒绝;视为基础设施失败,由引擎按重试/熔断处理,不传给用户聚合函数
- **业务错误** — Action 自身返回 Err;传给 Aggregator 与 FSM 决策函数,由用户逻辑决定后果

两类失败必须在主机侧明确区分,不允许把 trap 折叠成普通错误回流给用户。

## 与其他 crate 的契约

- 与 `shiroha-core`:WIT 中的 FSM 描述类型必须可双向映射至 core 的 FSM 类型;字段缺失视作版本不兼容
- 与 `shiroha-wasm`:Host imports 的具体实现由 wasm crate 提供
- 与 `shiroha-engine`:engine 调用 wasm crate 的高层 API,不直接接触 WIT 绑定
