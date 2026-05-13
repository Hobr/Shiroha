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

是否拆为两个独立 world(主控版 / 节点版)以减小节点端体积,见 `open-questions.md`。

## 导出 (Exports) 概念组

| 概念组 | 用途 | 由谁调用 |
|---|---|---|
| 描述符 | 给主控读出 FSM 静态结构(状态、转移、Action、分发策略) | 主控 |
| 决策 | 给定当前状态与输入,给出下一状态 | 主控 |
| Action 实现 | 给定 ActionRef 名称与参数,执行并返回结果 | 节点(主控自托管 worker 时也调) |
| 聚合 (可选) | 用户自定义的聚合函数 | 主控 |

具体函数签名在 `.wit` 文件中确定;此处只锁定概念组的存在与调用方。

## 导入 (Imports) — 主机能力

Shiroha 对 WASM 提供的能力总集**保持小而稳定**。MVP 阶段建议范围:

- **log** — 层级化日志输出,落入主机的 tracing 体系
- **clock** — wall-clock / monotonic 时间
- **net** — 出站 HTTP,基于 reqwest,代理受主机配置控制
- **kv** — 作用域内的小型键值存储,用于 Action 间共享非状态机数据(作用域待定,见 `open-questions.md`)

更激进的能力(任意文件、子进程、原始 socket)默认**不开放**。任何新能力都需在 `wit-interfaces.md` 显式声明其安全语义后方可加入。

## 能力授予粒度

每个 Flow 在上传时声明它需要哪些能力;主控在加载组件时按声明授予。授权超集的请求一律拒绝,避免静默扩权。

具体授权机制(配置文件 / 上传时 metadata / 控制面单独命令)待定。

## 部署与版本

- 用户 FSM 模块通过控制面上传到主控
- 主控保留组件字节、维护 Flow 的版本号
- 节点如何获得字节(预加载 vs 按需 pull)见 `open-questions.md`

## 失败语义

WIT 调用层面区分两类失败:

- **协议错误** — component trap、调用越界、能力拒绝;视为基础设施失败,由引擎按重试/熔断处理,不传给用户聚合函数
- **业务错误** — Action 自身返回 Err;传给 Aggregator 与 FSM 决策函数,由用户逻辑决定后果

两类失败必须在主机侧明确区分,不允许把 trap 折叠成普通错误回流给用户。

## 与其他 crate 的契约

- 与 `shiroha-core`:WIT 中的 FSM 描述类型必须可双向映射至 core 的 FSM 类型;字段缺失视作版本不兼容
- 与 `shiroha-wasm`:Host imports 的具体实现由 wasm crate 提供
- 与 `shiroha-engine`:engine 调用 wasm crate 的高层 API,不直接接触 WIT 绑定
