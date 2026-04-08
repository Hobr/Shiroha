# simple

一个最小的 Shiroha Flow component 示例，目标平台是 `wasm32-wasip2`。

它实现了仓库里的 [flow.wit](/mnt/data/Project/Shiroha/crate/shiroha-wit/wit/flow.wit)：

- `get-manifest`
- `invoke-action`
- `invoke-guard`
- `aggregate`

## 行为

- 初始状态：`pending-approval`
- `approve` 事件：
  先跑 guard `allow-approve`，成功后执行 action `ship`，进入终态 `approved`
- `reject` 事件：
  直接进入终态 `rejected`

`aggregate` 也给了一个最小 fan-out 聚合示例，但这份 simple Flow 自己并没有声明 `fan-out` action，因此这里只是展示 guest 侧聚合函数的写法：

- 当聚合函数名为 `pick-success` 且至少一个节点成功时，返回事件 `done`
- 否则返回事件 `retry`

## 构建

```bash
cargo build \
  --offline \
  --manifest-path example/simple/Cargo.toml \
  --target wasm32-wasip2 \
  --release
```

输出文件:

```bash
example/simple/target/wasm32-wasip2/release/simple.wasm
```

## 部署

```bash
sctl flow deploy \
  --file example/simple/target/wasm32-wasip2/release/simple.wasm \
  --flow-id approval-demo
```

部署后可以先确认服务端看到的 manifest：

```bash
sctl flow get --flow-id approval-demo
```

## 触发测试

创建一个带上下文的 Job，然后用带 payload 的事件推进：

```bash
sctl job new --flow-id approval-demo --context-text "demo-request"
sctl job trig --job-id <job-id> --event approve --payload-text "approved-by-cli"
```

等待进入终态并查看事件日志：

```bash
sctl job wait --job-id <job-id> --state completed
sctl job logs --job-id <job-id> --pretty
```
