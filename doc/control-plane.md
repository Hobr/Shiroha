# 控制面 (shiroha-control)

## 角色

控制面是 `sctl` 与 `shirohad` 之间的接口,语义是"管理这台主控"。未来的 TUI、Web UI 都将作为控制面的另一类客户端接入,共享同一组 RPC 定义。

控制面与节点面 transport **共用 gRPC 基础设施,但不复用同一组 proto**——两者关注的对象、版本节奏、鉴权策略完全不同,合并会导致一边的演进不必要地绑定另一边。

注意:控制面是 **Flow 概念唯一对用户可见的接口面**。Flow 是主控层的人类语义包装(name + version + 能力声明),引用底层的 `ComponentId`(主控对 WASM 字节做内容 hash 去重的主键)。控制面命令以 Flow 为粒度,但 worker / transport / dispatch 都不感知它,详见 `storage.md`。

## 命令分类

控制面对外暴露的命令大致归为四类:

| 类别 | 命令例 | 语义 |
| --- | --- | --- |
| Flow 管理 | upload-flow / list-flows / inspect-flow / delete-flow | 管理 FSM 定义。delete-flow 默认拒绝引用尚有非终态 Job 的 Flow;`--force` 取消相关 Job 后删除 |
| Job 管理 | create-job / list-jobs / inspect-job / cancel-job / pause-job / resume-job | 管理运行实例 |
| 观测 | tail-events / get-job-state / job-event-history | 实时与历史查询 |
| 节点管理 | list-nodes / drain-node / inspect-node | 维护节点拓扑 |

具体命令清单与字段在 proto 定义阶段细化;此处只锁定分类。具体每个动作的语义详见 `engine.md`。

## 接入点

- `sctl` 是首个客户端,通过 clap CLI 解析参数后调用控制面 gRPC
- 守护进程本地访问通过 **Unix domain socket**(默认场景)
- 远程访问通过 TCP + mTLS(鉴权细节见下方"鉴权与安全"节)
- 未来 TUI / Web UI 复用同一组 proto;Web UI 在前端经由网关层(可独立部署)将 HTTP 翻译为控制面 gRPC

## 流式响应

`tail-events`、长查询等天然需要 server-streaming。Proto 在初期就要规划好流式 RPC,避免后期破坏性变更。

游标语义由 storage 提供并向上透传到控制面,客户端可断流后用游标续上。

## 与 Engine 的边界

`shiroha-control` 仅做**协议层翻译**:

- 把入站请求映射为 Engine 的内部命令
- 把 Engine 返回值映射回 protobuf
- 处理流式响应的背压

控制面 crate 内**不写业务逻辑**;Engine 不感知 RPC 协议。这种分层让未来增加非 gRPC 控制面(直接 IPC / 文件 watcher / Web WebSocket)时,Engine 一行不动。

## 删除与级联

Flow 的删除可能触发级联:Job 的处置、节点缓存的清理。控制面**不自行决定**级联策略——它把删除请求转发给 Engine,Engine 根据当前状态决定接受、拒绝或要求 `--force`。这一职责划分避免 Storage / Engine / Control 三方就同一语义各执一词。

## 鉴权与安全

- **本地 UDS** — 受文件系统权限保护,默认场景下无显式鉴权
- **远程 TCP** — 需要 mTLS;客户端身份由证书决定;粗粒度授权(只读 / 读写 / 管理)在 MVP 后引入
- **令牌** — 长期未定,先用 mTLS 作为唯一鉴权机制

具体细节列入后续工作,不阻塞首个可用版本。

## 与其他 crate 的契约

- 依赖:`shiroha-core`、`shiroha-engine`、`shiroha-storage`(只读查询)、`shiroha-proto`
- 被依赖:`apps/shirohad`(装配)、`apps/sctl`(客户端 stub)
- 不依赖:`shiroha-transport`(节点面)、`shiroha-dispatch`、`shiroha-wasm`
