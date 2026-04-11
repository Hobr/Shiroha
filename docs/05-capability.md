# 能力与权限系统

## 声明

WASM 模块在状态机定义中声明依赖的能力，区分 **必需** 和 **可选**。

## 校验流程

Controller 维护能力策略（允许/拒绝列表），部署 WASM 时：

1. 提取能力清单
2. 按部署策略计算授权结果
3. 必需能力未授权 → 拒绝部署
4. 可选能力未授权 → 标记为不可用桩，调用时返回错误而非崩溃
5. 生成不可变的 `deployment manifest`，其中至少包含：
   - `deployment_id`
   - `wasm_hash`
   - WIT / 执行契约版本
   - 已授权能力集合
   - 被拒绝的可选能力集合

## 授权结果与执行期凭证

- `deployment manifest` 只记录不可变授权结果，不要求当前阶段固定承载执行期原始密钥或具体句柄字段
- capability policy 负责决定“允许使用什么能力”；如实现需要，可通过 capability materialization 或等价 binding 机制决定“节点实际拿到什么短期句柄或凭证”
- 若能力语义、授权集合和执行契约不变，执行期凭证可以独立轮换，不必因此生成新的 `deployment_id`
- 若能力语义或授权边界发生变化，应通过新的 deployment 与新的 manifest 表达，而不是在旧 deployment 上静默替换

## 节点侧执行

- Task 不只携带 `wasm_hash`，还引用 `deployment_id`
- Node 必须按 `deployment manifest` 执行，而不是重新解释一套本地授权规则
- 若实现需要执行期 binding，Node 应只为当前 `deployment_id` 与当前 `attempt` materialize 最小权限的短期 runtime handle
- 若 Node 无法满足 manifest 要求，应在执行前拒绝 task，而不是静默放宽或缩小权限
- 若 Node 无法获得执行所需的短期句柄或凭证，应在执行前显式拒绝 task，而不是回退到更宽松的本地默认配置

## 扩展方式

新增能力只需：添加 WIT interface → SDK 封装 → engine 注册链接器 → 在 manifest 中加入对应授权项。不影响现有模块。
