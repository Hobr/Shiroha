# WIT 接口设计

文件位置：`wit/shiroha.wit`，包命名：`shiroha:host`。

## Guest 世界 (用户 WASM 实现)

`world shiroha-guest` 导出一组与状态机相关的接口：

- 读取 FSM 定义：返回状态集合、转换表、Action 元数据 (含分发与聚合声明)。
- 状态钩子：进入状态、离开状态。
- 守卫：在转换前进行条件判断。
- Action：业务侧的重运算入口，输入为字节序列，输出为字节序列，语义由用户自定。

用户只需实现 FSM 结构与 Action 函数本身。分发与聚合以**元数据**形式声明，不在 guest 内执行。

## Host 世界 (主机提供给 guest 使用)

`world shiroha-host` 向 guest 暴露 WASI 未覆盖或需要显式控制的能力：

- 日志 (结构化输出到宿主追踪系统)
- 键值存储 (只读或带命名空间的读写)
- HTTP 客户端 (受宿主策略限制)
- 时钟与计时
- 随机数
- 指标上报

每种能力都是独立的 WIT interface，guest 可按需 import，保持最小权限。

## 自定义聚合

当某个 Action 声明 `AggregateSpec::Reduce` 时，reduce 函数也是 guest 的一个导出。主控在独立的 Wasmtime 沙箱中调用它，输入是本次 Action 的结果流，输出是单一合并结果。与执行 Action 的沙箱隔离，不共享内存。

## 版本与兼容

- WIT 文件纳入版本管理；每次破坏性变更须升主 版本。
- 主控在加载 WASM 时校验导出世界与期望版本匹配。
