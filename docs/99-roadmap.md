# Roadmap

> 本 Roadmap 顺序以“先闭环、再分布式、后产品化”为原则，而不是以 crate 实现顺序为原则。

## 规划原则

- 先冻结执行契约，再写运行时代码
- 先跑通单机闭环，再引入远程节点
- 先保证恢复与可观测性，再追求更丰富的策略和更复杂的部署形态
- 先把一条参考实现跑通，再从已验证路径抽象 transport / storage / executor 接口
- 当前阶段仅支持宿主内建的 dispatch / aggregation 策略

## 阶段 1：Contract Freeze

目标：把 Guest / Host / Controller / Node 之间的边界固定下来，避免后续边写边改协议。

范围：

- 固定 `WIT` world、共享类型和最小能力集
- 固定 `deployment manifest`、`deployment_id`、`task`、`attempt` 等核心模型
- 固定内建 dispatch / aggregation 策略的声明方式与限制
- 明确错误模型、版本边界和最小兼容承诺

完成标志：

- 可以编译一个最小 Guest 示例模块
- Controller 能静态解析 definition 并生成 `deployment manifest`
- 文档中不再存在关键语义空白点，尤其是授权、版本、task 生命周期与聚合约束

## 阶段 2：Single-Node Kernel

目标：先把单节点下的核心执行引擎做正确，不引入分布式复杂度。

范围：

- `engine` 完成模块加载、能力注入、Action 执行
- `machine` 完成 Effect 驱动的状态流转
- `dispatch` 先只支持本地执行路径，但必须使用真实 `task` / `attempt` 语义
- `storage` 落地最小持久化模型：deployment、instance、task、attempt、event log

完成标志：

- 单进程内可以完成：部署模块、创建实例、执行 Action、持久化状态、恢复后继续运行
- 本地执行与文档中的 task 生命周期一致
- 关键失败路径有测试：超时、重试、取消、聚合失败、恢复

非目标：

- 远程节点
- 节点注册
- 网络传输
- 提前为多 transport / 多 storage 后端做通用化

## 阶段 3：Standalone Alpha

目标：提供一个真正可操作的单机版本，用它验证整体产品形态，而不是只验证库。

范围：

- `shirohad` 跑通 standalone 模式
- `sctl` 提供最小管理能力：部署、实例创建、实例查询、task 查询、日志查看
- 补齐基本观测性：结构化日志、错误码、关键状态转移日志
- 打通一个完整示例状态机，作为回归样例

完成标志：

- 用户可以只启动一个进程就完成完整工作流
- 可以通过 CLI 观察 deployment、instance、task 的状态变化
- 至少有一个端到端示例覆盖部署、执行、失败恢复

非目标：

- 多节点调度优化
- 复杂权限策略管理界面
- Web / TUI

## 阶段 4：Remote Execution Alpha

目标：在不改变单机执行语义的前提下，把 task 安全地分发给远程节点。

范围：

- 引入 Controller / Node 两种运行模式
- 实现 gRPC 控制面与任务面协议
- 实现节点注册、心跳、模块拉取、结果回传
- 远程执行仍以当前内建 dispatch / aggregation 策略为准
- 远程路径必须沿用相同的 `deployment manifest`、`task`、`attempt` 模型

完成标志：

- Controller 能将 task 分发到远程 Node 并收回结果
- Node 能按 `deployment manifest` 校验并执行模块
- Controller 重启后，未完成 task 可以恢复或重试
- `Local`、`RemoteAny`、`RemoteAll(n)` 在支持范围内行为一致

非目标：

- 动态扩缩容策略
- 多传输实现
- 自定义 dispatch / aggregation 算法

## 阶段 5：Reliability Beta

目标：把“能跑”提升到“遇到故障也不会失控”。

范围：

- 完善租约、超时、重试、取消与节点失联处理
- 增强认证与传输安全
- 补齐审计信息、节点健康状态和执行统计
- 建立兼容性检查与升级策略，确保旧实例不会因宿主升级被破坏

完成标志：

- 节点失联、重复回报、Controller 重启、模块缓存失效等场景有明确定义和测试
- 具备最小安全基线：节点身份校验、传输加密、模块哈希校验
- 版本兼容策略可执行，而不是只停留在文档表述

## 阶段 6：Developer Preview

目标：让外部开发者可以稳定地基于 Shiroha 编写和调试 Guest 模块。

范围：

- 打磨 Guest SDK、宏与示例项目
- 补齐开发文档、部署文档和运维文档
- 提供更顺手的 CLI 体验
- 明确支持矩阵与已知限制

完成标志：

- 新开发者可以在无口头说明的情况下完成示例开发与部署
- 文档覆盖部署、执行模型、故障语义、限制项
- 至少有一条推荐开发路径和一条推荐部署路径

## 长期规划

- Guest 通过 WASM 自定义 dispatch / aggregation 策略
- QUIC、消息队列等额外传输实现
- TUI / Web 管理界面
- 更细粒度的 capability policy 与多租户隔离

## 当前建议的执行顺序

1. 先完成阶段 1，冻结契约与数据模型
2. 然后完成阶段 2 和阶段 3，拿到可验证的单机闭环
3. 之后进入阶段 4，验证分布式路径是否与单机语义一致
4. 最后做阶段 5 和阶段 6，把系统从原型推到可对外使用
