# advanced

一个更完整的 `wasm32-wasip2` Shiroha Flow component 示例，展示三类声明：

- `timeout`
- `fan-out`
- `subprocess`

它实现了仓库里的 [flow.wit](/mnt/data/Project/Shiroha/crate/shiroha-wasm/wit/flow.wit)。

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
- `fan-out`
  当前主要用于展示 manifest / guest 侧写法，完整多节点分发仍待继续实现
- `subprocess`
  当前主要用于展示 manifest 写法，父子 Job 自动编排仍待继续实现

也就是说，这份示例是“面向完整设计”的 component 样例，不是说这三类路径现在都已在 Shiroha 里完全执行完毕。

## 构建

```bash
cargo build \
  --offline \
  --manifest-path examples/advanced/Cargo.toml \
  --target wasm32-wasip2 \
  --release
```

输出文件：

```bash
examples/advanced/target/wasm32-wasip2/release/advanced.wasm
```

## 部署

```bash
sctl deploy \
  --file examples/advanced/target/wasm32-wasip2/release/advanced.wasm \
  --flow-id advanced
```
