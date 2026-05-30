# Roadmap

本文档把 `doc/` 中描述的 v1.0 终点拆成 7 个 minor 版本里程碑。每版都是**可交付状态**:跑得起来、有测试、可以停下来用一段时间;后续版本只在前版基础上叠加,不破坏已有契约。

起始版本 v0.3.0(当前 `Cargo.toml` 已锁定),首个生产版本 v1.0.0。

## 节奏速览

| 版本 | 主题 | 新引入 crate |
| --- | --- | --- |
| v0.3.0 | FSM 内核 + WASM 接入(单进程,内存态) | core / wit / wasm / engine |
| v0.4.0 | 持久化 + 控制面 + CLI(单机完整可用) | storage / proto-control / control / config / apps |
| v0.5.0 | 分布式基础(远程节点,N=1 派发) | transport / transport-grpc / proto-node / worker / dispatch |
| v0.6.0 | 多节点聚合 + Waiting 模式 + 定时器 | — |
| v0.7.0 | 失败健壮性(回滚 / 恢复 / 级联 / pause-resume) | — |
| v0.8.0 | 安全(mTLS)+ 可观测(tracing / metrics) | — |
| v0.9.0 | 性能基准 + API 冻结 + 用户文档 | — |
| v1.0.0 | GA(bug 修复 + 文档校对 + 发布流程) | — |

## v0.3.0 — FSM 内核 + WASM 接入

**主题**:把状态机模型与 WASM 组件接入跑通,其它一切暂不存在。

**新增 crate**:`shiroha-core`、`shiroha-wit`、`shiroha-wasm`、`shiroha-engine`(内存驱动,无 Store 抽象)

**新增能力**:

- core 的 FSM / ActionRef / DispatchPolicy / Aggregation 类型与 trait;**Executor trait**定义
- 加载 WASM component,读取 `describe` / `decide` / `action` 导出
- 主机能力:`log` + `clock`(其它能力 v0.4+ 再加)
- 引擎在内存中跑 Job,主控关闭即丢
- Engine 直接调用 wasm 的 LocalExecutor(不经过 dispatch 层;dispatch 作为第二个 Executor 实现场景在 v0.5 引入)
- `WaitingMode = Blocking` only

**不在范围**:持久化、网络、CLI、远程节点、Waiting 模式、Quorum/Custom 聚合、dispatch 层

**出口标准**:

- 一份 WASM component(2–3 状态、若干 Action)能被加载并跑到终态
- `cargo test --workspace` 全绿,含至少一个端到端集成测试
- **边界测试**:非法 FSM 描述(循环无出口状态)被拒绝并返回明确错误;空 on-enter/on-exit 列表的状态转移正常执行;describe export 返回不合法结构时的错误处理

## v0.4.0 — 持久化 + 控制面 + CLI

**主题**:单机使用变完整。重启不丢状态;sctl 可操作整个生命周期。

**新增 crate**:`shiroha-storage`、`shiroha-proto-control`、`shiroha-control`、`shiroha-config`、`apps/shirohad`、`apps/sctl`

**新增能力**:

- Store trait + redb 实现;**QueryStore** 只读子 trait(供 control 使用);Component / Flow / Job / Event 持久化
- engine 从内存驱动重构为 Store-backed
- 主控重启后恢复未终态 Job 的驱动循环
- 控制面 gRPC over UDS
- sctl:`upload-flow` / `list-flows` / `inspect-flow` / `create-job` / `list-jobs` / `inspect-job` / `cancel-job` / `tail-events` / `get-job-state`
- 主机能力补齐:`kv` (per-Job)、`fs.readonly` (白名单)、`rand` (CSPRNG)

**不在范围**:`delete-flow` 级联 / `--force`、`pause-job` / `resume-job`、远程节点、`net.http`

**出口标准**:

- 完整冒烟流程通过:`sctl upload-flow → create-job → tail-events → 观察终态`
- `kill -9` shirohad,重启后所有未终态 Job 自动恢复
- redb 文件能跨版本读取(为后续兼容打基础)

## v0.5.0 — 分布式基础

**主题**:第一次出现远程节点;Action 派单到远端。

**新增 crate**:`shiroha-transport`、`shiroha-transport-grpc`、`shiroha-proto-node`、`shiroha-worker`、`shiroha-dispatch`

**新增能力**:

- Transport trait + NodeRegistry(静态配置,从 config 读节点列表)
- gRPC transport 实现:`submit-action` / `cancel-action` / `fetch-component` / `heartbeat`
- worker 端:按 ComponentId 缓存 + 按需 pull
- **dispatch 层引入**:Dispatcher + Aggregator;`DispatchPolicy::Local` + `DispatchPolicy::Remote(selector=one(...), agg=First)`
- Engine 从直接调 wasm 重构为经由 dispatch 层(Executor trait 的第二个实现场景)
- 主控自托管 worker 走 LocalExecutor,避免自环 RPC
- 主机能力补:`net.http`(GET + POST)

**不在范围**:多节点聚合(N>1)、`WaitingMode = Waiting`、节点动态注册、mTLS

**出口标准**:

- 同机:shirohad master + shirohad worker(独立进程)跑通端到端
- 异机:Action 在 worker 上执行,结果回到 master
- worker 重启后,缓存为空,首次执行自动 pull Component

## v0.6.0 — 多节点聚合 + Waiting 模式

**主题**:把分发与同步性补齐到 v1 设计形态。

**新增能力**:

- Selector 补齐:`n(count, filter)` + `all(filter)`
- Aggregation 补齐:`Quorum(k)` + `Custom`(WASM `aggregate` export)
- Aggregator 提前返回 + 沿 transport 取消 + 后台兜底(晚到结果写 dead letter)
- `WaitingMode = Waiting` 完整路径:同事务写入 `Waiting(action_id)` + 释放驱动循环 + 结果回流唤醒
- 定时器:Action 超时、状态停留超时、Job 寿命超时
- WIT 的 `aggregate` export 概念组上线

**不在范围**:节点动态注册、安全、性能调优

**出口标准**:

- `Remote(n(3), Quorum(2))` 在三节点上跑通;杀掉一个节点不影响 Job
- 长 Action(>30s)用 Waiting 模式;Action 中途主控重启,Job 能继续推进至终态
- Action 超时触发 ActionRef 失败路径

## v0.7.0 — 失败健壮性

**主题**:把所有"边缘路径"补齐;故障注入测试通过。

**新增能力**:

- Job 创建失败显式回滚(删 Job + append `CreationFailed` Event)
- 转移失败显式回滚
- Blocking 模式主控重启:在途 Action 一律失败,走 ActionRef 失败路径(无节点结果缓存)
- `delete-flow` + `--force` 级联(取消相关 Job 后删 Flow)
- `pause-job` / `resume-job`
- Event 保留:Job 终态后 N 天清理(默认 30 天,配置项可调);`export-events` 导出命令
- 节点离线 → 重连的 NodeRegistry 状态恢复

**不在范围**:跨版本 Job 迁移(保留为永久未来项)、安全、性能

**出口标准**:

- Chaos 测试(网络中断 / 主控 SIGKILL / 节点 SIGKILL 各 10 轮)无状态紊乱
- `delete-flow` 在有非终态 Job 时拒绝;加 `--force` 后成功
- pause 中的 Job 不消费新触发,resume 后从最近持久化状态继续

## v0.8.0 — 安全 + 可观测性

**主题**:让框架能进生产候选环境。

**新增能力**:

- 远程链路 mTLS(节点面 + 控制面),自签 CA 流程文档化
- 配置驱动的能力授予粒度(Flow 能力声明 + 主控校验,声明外的能力调用 trap)
- tracing 结构化日志(JSON 输出,支持 OTLP 导出)
- 关键指标:Job 数与状态分布、转移延迟分位、Action 失败率、节点健康度、storage 文件大小
- sctl 支持 `--server <endpoint>` 跨网络访问(走 mTLS)

**不在范围**:客户端粒度授权(只读 / 读写 / 管理),完整审计日志

**出口标准**:

- 跨主机 mTLS 通信正常,自签 CA 一键脚本可用
- 一份示例 Grafana / Prometheus 配置(或同等观测面板)进入仓库
- 三类典型故障(节点离线 / 网络抖动 / Action 失败率突增)可在指标上直接看到

## v0.9.0 — 性能 + 稳定 API + 用户文档

**主题**:API 冻结、性能 baseline、用户文档完工。

**新增能力**:

- 性能基准(在文档化硬件上):状态转移吞吐、Action 派发延迟分位、storage 写入 QPS
- 公开 API 的 SemVer 承诺:`shiroha-core` 全部公开类型、WIT 接口、控制面 proto、节点面 proto
- 用户文档:写 FSM 模块的指南、部署 cookbook、能力清单参考、一个完整示例项目
- `cargo deny` / `cargo audit` 基线进入 CI

**不在范围**:深度性能优化(性能修复以 patch 版本节奏发出)

**出口标准**:

- Bench 结果对比上一次基线无回归 ≥ 10%
- 用户文档覆盖至少一个端到端示例项目,从零跑通
- 标记为 Release Candidate

## v1.0.0 — GA

**主题**:首个正式发布。

- v0.9 RC 通过后,bug 修复 + 文档校对 + 发布流程
- 第一个 `git tag v1.0.0`
- Release notes、迁移指南(若有)、感谢列表

## 跨版本约束

- 后续版本**不允许破坏前一版本的核心抽象**:`shiroha-core` 的公开 API 一旦在某版本冻结,下一 minor 只能扩,不能改
- 每个 PR 描述应注明属于哪一版本里程碑
- 不在当前版本里程碑里的 feature 不允许提前合入(避免范围蔓延)
- 抽象的引入跟随第二个实现:第一个使用点直接硬编码,第二个使用点出现时再提 trait
