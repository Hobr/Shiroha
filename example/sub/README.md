# sub

这是一组父子 Flow component 示例：

- 父流程：
  [parent/src/lib.rs](/mnt/data/Project/Shiroha/example/sub/parent/src/lib.rs)
- 子流程：
  [child/src/lib.rs](/mnt/data/Project/Shiroha/example/sub/child/src/lib.rs)

两者都实现了 [flow.wit](/mnt/data/Project/Shiroha/crate/shiroha-wasm/wit/flow.wit)，目标平台都是 `wasm32-wasip2`。

## 设计关系

父流程 `purchase-parent-demo`：

- 初始状态 `draft`
- `submit` 后进入 `legal-review`
- `legal-review` 是 `subprocess` 状态
  `flow-id = legal-review-demo`
  `completion-event = legal-review-complete`
- 收到 `legal-review-complete` 后进入终态 `approved`
- 收到 `legal-review-rejected` 后进入终态 `rejected`

子流程 `legal-review-demo`：

- 初始状态 `review-pending`
- `approve` 后进入 `approved`
- `reject` 后进入 `rejected`

## 构建

父流程：

```bash
cargo build \
  --offline \
  --manifest-path example/sub/parent/Cargo.toml \
  --target wasm32-wasip2 \
  --release
```

子流程：

```bash
cargo build \
  --offline \
  --manifest-path example/sub/child/Cargo.toml \
  --target wasm32-wasip2 \
  --release
```

## 部署顺序

先部署子流程，再部署父流程：

```bash
sctl deploy \
  --file example/sub/child/target/wasm32-wasip2/release/child.wasm \
  --flow-id legal-review-demo

sctl deploy \
  --file example/sub/parent/target/wasm32-wasip2/release/parent.wasm \
  --flow-id purchase-parent-demo
```

## 当前运行时说明

这组示例表达的是“最终想要的父子流程关系”。目前仓库里：

- component guest 已经可部署和执行
- `subprocess` 的 manifest 声明已经可用
- 但“进入 subprocess 状态后自动创建子 Job，再在子 Job 完成时回注 completion-event”这条运行时链路还没有完全实现

所以当前你可以：

1. 部署这两个 component
2. 你可以先单独创建一个子流程 Job，手工触发 `approve` 或 `reject`，确认子流程本身可运行
3. 创建父流程 Job，并用带 payload 的 `submit` 让父流程进入 `legal-review`
4. 先手工触发父 Job 的 `legal-review-complete` 或 `legal-review-rejected`，模拟子流程结果回注

示例命令：

```bash
sctl create --flow-id legal-review-demo
sctl trigger --job-id <child-job-id> --event approve

sctl create --flow-id purchase-parent-demo
sctl trigger --job-id <parent-job-id> --event submit --payload-text "legal-review-request"
sctl get --job-id <parent-job-id>
sctl trigger --job-id <parent-job-id> --event legal-review-complete
sctl wait --job-id <parent-job-id> --state completed
sctl events --job-id <parent-job-id> --pretty
```

也就是说，这是一组“真实可编译 component + 真实父子流程建模示例”，但自动父子联动本身还需要继续开发。
