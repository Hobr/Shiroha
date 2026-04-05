# Shiroha

> 由 WebAssembly 驱动的分布式状态机任务编排框架

## 环境准备

```bash
rustup toolchain install
rustup target add wasm32-wasip2
cargo install just cargo-binstall

# Ubuntu / Debian
sudo apt update
sudo apt install -y protobuf-compiler libprotobuf-dev pre-commit

# 安装开发工具:
just install-dev
```

## 快速开始

### 1. 编译二进制和示例 Flow

```bash
just build
```

### 2. 启动本地服务端

默认会把数据写到 `./data/shiroha.redb`:

```bash
just shirohad --listen 127.0.0.1:50051 --data-dir ./data
```

### 3. 部署一个最小 Flow

构建最小示例 component:

```bash
just build-example-simple
```

然后用 `sctl` 部署:

```bash
just sctl --server http://127.0.0.1:50051 flow deploy \
  --flow-id simple \
  --file example/simple/target/wasm32-wasip2/release/simple.wasm
```

查看部署后的 manifest 和拓扑摘要:

```bash
just sctl --server http://127.0.0.1:50051 flow get --flow-id simple
just sctl --server http://127.0.0.1:50051 flow get --flow-id simple --summary
just sctl --server http://127.0.0.1:50051 flow vers --flow-id simple
```

### 4. 创建 Job 并驱动状态机

创建一个 Job:

```bash
just sctl --server http://127.0.0.1:50051 job new \
  --flow-id simple \
  --context-text "demo-request"
```

列出当前 Job, 拿到 `job_id`:

```bash
just sctl --server http://127.0.0.1:50051 job ls --all
just sctl --server http://127.0.0.1:50051 job get --job-id <job-id>
```

触发 `approve` 事件并等待结束:

```bash
just sctl --server http://127.0.0.1:50051 job trig \
  --job-id <job-id> \
  --event approve \
  --payload-text "approved-by-cli"

just sctl --server http://127.0.0.1:50051 job wait \
  --job-id <job-id> \
  --state completed
```

查看事件日志:

```bash
just sctl --server http://127.0.0.1:50051 job logs --job-id <job-id> --pretty
```

如果要让脚本消费输出, 给任意命令加 `--json`:

```bash
just sctl --server http://127.0.0.1:50051 --json flow ls
just sctl --server http://127.0.0.1:50051 --json job get --job-id <job-id>
```

### 5. 删除测试数据

```bash
just sctl --server http://127.0.0.1:50051 job rm --job-id <job-id>
just sctl --server http://127.0.0.1:50051 flow rm --flow-id simple
```

如果 Job 仍在运行, 可先取消, 或直接强制删除:

```bash
just sctl --server http://127.0.0.1:50051 job cancel --job-id <job-id>
just sctl --server http://127.0.0.1:50051 job rm --job-id <job-id> --force
just sctl --server http://127.0.0.1:50051 flow rm --flow-id simple --force
```

## CLI 使用概览

`sctl` 的全局参数:

- `--server`: 指定 `shirohad` 地址, 默认 `http://[::1]:50051`
- `--json`: 输出稳定 JSON, 便于脚本消费

常用命令:

- `flow deploy | ls | get | vers | rm`
- `job new | ls | get | trig | pause | resume | cancel | logs | wait | rm`
- `complete`: 生成 shell 补全脚本

示例:

```bash
just sctl complete fish --install
just sctl complete zsh --print-path
```

## 示例 Flow

- `example/simple`
  最小可运行示例, 适合快速验证部署、创建 Job、触发事件
- `example/advanced`
  展示 `timeout`、`fan-out`、`subprocess` 的完整 manifest 建模
- `example/warning-deadlock`
  一个会触发 FlowValidator warning 的负例
- `example/sub`
  父子 Flow 建模示例, 当前可用于手工模拟 `subprocess` 回注

各示例目录都有单独的 `README.md` 说明构建和测试方式

## 开发说明

- 代码格式化和 pre-commit 检查: `just fmt`
- 工作区编译检查: `just check`
- 全量测试: `just test`
- API 文档: `just doc`

更完整的架构和路线图见:

- `docs/architecture.md`
- `docs/core-concepts.md`
- `docs/wasm-design.md`
- `docs/roadmap.md`
