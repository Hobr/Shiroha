# warning-deadlock

一个“可以正常编译、可以被部署，但会触发 FlowValidator warning”的负例组件。

它故意包含这些问题：

- `idle` / `loop` 两个可达状态都无法到达任何终态
- `done` 是不可达终态

因此：

- `cargo build` 会成功
- `sctl flow deploy` 会成功
- `shirohad` 日志里会出现 validation warnings

## 构建

```bash
cargo build --offline \
  --manifest-path example/warning-deadlock/Cargo.toml \
  --target wasm32-wasip2 \
  --release
```

输出文件：

```bash
example/warning-deadlock/target/wasm32-wasip2/release/warning_deadlock.wasm
```

## 本地测试

先启动服务端并打开 warning 日志：

```bash
RUST_LOG=warn cargo run -p shirohad -- --listen 127.0.0.1:50051
```

然后部署：

```bash
sctl flow deploy \
  --file example/warning-deadlock/target/wasm32-wasip2/release/warning_deadlock.wasm \
  --flow-id warning-deadlock
```

预期结果：

- CLI 部署成功
- `shirohad` 日志里会看到 `flow validation warnings`
- warning 文案会包含：
  - `state 'idle' cannot reach any terminal state`
  - `state 'loop' cannot reach any terminal state`
  - `state 'done' is unreachable from initial state`
