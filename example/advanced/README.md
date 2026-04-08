# advanced

一个更完整的 `wasm32-wasip2` Shiroha Flow component 示例，展示三类声明：

- `timeout`
- `fan-out`
- `subprocess`

它实现了仓库里的 [flow.wit](/mnt/data/Project/Shiroha/crate/shiroha-wit/wit/flow.wit)。

## Flow 结构

状态：

- `draft`
- `legal-review`
  `kind = subprocess`
- `quote-collection`
- `waiting-approval`
- `approved`
- `rejected`
- `timed-out`

转移：

- `draft --submit--> legal-review`
  guard: `has-minimum-payload`
  action: `normalize-request`
- `legal-review --legal-review-complete--> quote-collection`
- `quote-collection --collect-quotes--> waiting-approval`
  action: `collect-quotes`
  dispatch: `fan-out(count=3, aggregator=pick-success)`
- `waiting-approval --approve--> approved`
  guard: `allow-approve`
  action: `ship`
- `waiting-approval --reject--> rejected`
- `waiting-approval --expire--> timed-out`
  timeout: `30_000ms`

## 代码里演示了什么

- `normalize-request`
  演示本地 action
- `collect-quotes`
  演示 `fan-out` action 的声明方式
- `pick-success`
  演示聚合函数如何根据多个 `NodeResult` 返回事件
- `legal-review`
  演示 `subprocess` 状态如何在 manifest 中声明

## 当前 runtime 限制

这份示例里的三种能力里，含义不完全相同：

- `timeout`
  当前 standalone 路径已经能真正跑通
- `guard` / `local` / `remote`
  当前 standalone 路径已经能真正跑通；`remote` 当前会通过 in-process transport 进入同进程 node worker
- `fan-out`
  当前 standalone 已经能在同进程 fan-out 槽位上执行、聚合并回注 follow-up event，但这仍不是一个真实多节点集群
- `subprocess`
  当前主要用于展示 manifest 写法，父子 Job 自动编排仍待继续实现

也就是说，这份示例是“面向完整设计”的 component 样例，不是说这三类路径现在都已在 Shiroha 里完全执行完毕。

## 构建

```bash
cargo build \
  --offline \
  --manifest-path example/advanced/Cargo.toml \
  --target wasm32-wasip2 \
  --release
```

输出文件：

```bash
example/advanced/target/wasm32-wasip2/release/advanced.wasm
```

## 部署

```bash
sctl flow deploy \
  --file example/advanced/target/wasm32-wasip2/release/advanced.wasm \
  --flow-id advanced-orchestration-demo
```

## 当前可实际跑通的路径

当前这份 component 里：

- `submit -> legal-review` 这一段已经能真实执行
- `fan-out` 运行时本身已经能工作
- 但这份示例里的 `aggregate()` 当前返回 `quotes-collected` / `quote-failed`，而 manifest 没有为 `waiting-approval` 定义对应出边，所以它还不是一个“按现状可直接跑通到终态”的 fan-out 样例
- 自动 `subprocess` 编排仍未落地

因此当前最稳妥、可实际跑通的链路是前半段：

```bash
sctl job new --flow-id advanced-orchestration-demo --context-text "quote-request"
sctl job trig --job-id <job-id> --event submit --payload-text "draft-ready"
```

此时 Job 会从 `draft` 进入 `legal-review`，并执行 `normalize-request` action。可以用下面命令确认：

```bash
sctl job get --job-id <job-id>
sctl job logs --job-id <job-id> --pretty
```
