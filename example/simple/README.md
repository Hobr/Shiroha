# simple

一个最小的 Shiroha Flow component 示例，目标平台是 `wasm32-wasip2`。

它实现了仓库里的 [flow.wit](/mnt/data/Project/Shiroha/crate/shiroha-wasm/wit/flow.wit)：

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

`aggregate` 也给了一个最小 fan-out 聚合示例：

- 当聚合函数名为 `pick-success` 且至少一个节点成功时，返回事件 `done`
- 否则返回事件 `retry`

## 构建

```bash
cargo build \
  --offline \
  --manifest-path examples/simple/Cargo.toml \
  --target wasm32-wasip2 \
  --release
```

输出文件:

```bash
examples/simple/target/wasm32-wasip2/release/simple.wasm
```

## 部署

```bash
sctl deploy \
  --file examples/simple/target/wasm32-wasip2/release/simple.wasm \
  --flow-id simple
```

## 触发测试

```bash
sctl create --flow-id simple
sctl trigger --job-id <job-id> --event approve
```
