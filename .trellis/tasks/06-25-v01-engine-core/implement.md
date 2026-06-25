# v0.1 引擎内核 执行计划(implement.md)

> v0.1 child task `.trellis/tasks/06-25-v01-engine-core` 执行计划。`task.py start` 后依此推进。父任务产品决策见父 `prd.md`(D1/D3/D5/D7),技术设计见 child `design.md`,术语见父 `glossary.md`。

## 前置依赖

无。v0.1 = 父版本路线第一站,自起。

## 工作区结构(本次首次建工作区骨架)

按父 design §6 的 12-crate 布局,v0.1 只建 2 个 crate + bench,但**首次创建工作区虚拟 manifest `Cargo.toml` + `[workspace.dependencies]` 统一 pin** ——

```toml
# /Cargo.toml (workspace virtual manifest)
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.dependencies]
serde     = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
criterion = "0.5"           # dev/test only
# (后续版本才 pin: tokio / wasmtime / tonic / prost / opentelemetry* / tracing / anyhow / thiserror)
```

- `/crates/shiroha-ir/Cargo.toml`:`[dependencies] serde = { workspace = true }, serde_json = { workspace = true }`;`[dev-dependencies]` 不需。
- `/crates/shiroha-core/Cargo.toml`:`[dependencies] shiroha-ir = { path = "../shiroha-ir" }`;**零额外依赖**。
- `/benches/transition_latency.rs`:cargo bench harness,依赖 `shiroha-core`(经 `[dev-dependencies]` 在 `shiroha-core` 上)。
- v0.1 暂不建 `bin/`、`proto/`、其他 crate。

## 实现检查清单(按依赖顺序)

### A. 工作区骨架
- [ ] A1 创建 `/Cargo.toml`(virtual manifest)+ `/crates/shiroha-ir` + `/crates/shiroha-core` 两个 crate(`lib` 类型),`cargo build --workspace` 空实现能过。
- [ ] A2 `[workspace.dependencies]` pin serde/serde_json/criterion;各 crate 引用 `{ workspace = true }`。

### B. `shiroha-ir` 字段一次定对(G2 冻结)
- [ ] B1 按 child design §2 定义全部 struct/enum(`SmIr`/`StateNode`/`Region`/`StateKind`/`RegionKind`/`Transition`/`Expr`/`Cmp`/`ActionDecl`/`ActionRef`/`CapabilityDecl`/`HistoryDecl`/`HistoryDepth`/`HistoryRef`/`TargetSpec`/`AggregateRef`/`BuiltinAggregate`)。
- [ ] B2 `#[serde(tag, rename_all)]` / `#[serde(default)]` 逐字段过一遍,保证 serde round-trip 且加法兼容。
- [ ] B3 单元 `#[test] serde_roundtrip()`:手构 fixture IR → serialize → deserialize → assert eq。
- [ ] B4 `cargo tree -p shiroha-ir` 只出现 `serde`+`serde_json`(AC1)。

### C. `shiroha-core` 引擎
- [ ] C1 `CompiledIr`:从 `SmIr` 编译 → 预计算 `ancestors`/`exits`/`enters`/`lca`/`is_compound`/`is_final`;编译期校验(state refs 有效、initial 指向 region 内、不可达状态警告不阻断)。
- [ ] C2 `TaskInstance`:`active`(active state set)、`inflight`、`completed`、`events`、`history` slot。
- [ ] C3 `Engine` API:`new(create_instance/submit_event/poll_advance/take_completions/drive_completion`(签名同 design §3)。
- [ ] C4 RTC 步:select-transition / eval-guard / LCA / exit·run·enter / active set 更新(纯函数,非阻塞)。
- [ ] C5 浅历史:`exit` compound state 时记 active 直接子;`enter` 历史 pseudo 时恢复;`Deep` 字段遇时返回 `HistoryUnsupported` 错误(不静默错)。
- [ ] C6 `TransitionCache`:`HashMap<(StateRef, Option<String>), Vec<idx>>`,引擎一次 warmup 后命中率 ≥ 80%(深嵌套 fixture 验证)。
- [ ] C7 `ActionRuntime`/`CompletionSink` trait 定义;`TaskAction` / `Completion` / `PollOutcome` 类型。
- [ ] C8 `cargo tree -p shiroha-core` 只出现 `shiroha-ir`+`serde`(AC3)。

### D. 测试与 bench
- [ ] D1 IR fixtures(纯构造):扁平 / 嵌套 / 并行 / 浅历史 / guard 共 6+ 个 fixture。
- [ ] D2 引擎行为单测(AC4):compound entry/exit、orthogonal 并行激活、LCA 顺序、guard 拦截、跨域 fork/join、浅历史恢复、`final` 自动 completion。
- [ ] D3 异步动作回流单测(AC5):`MockActionRuntime` 注入 Done/Error → `drive_completion` → 下一转换被驱动。
- [ ] D4 `benches/transition_latency.rs`:三类 fixture(扁平/中等嵌套/深嵌套),criterion 默认参数,warmup 后测纯 RTC 段。
- [ ] D5 跑 `cargo bench --bench transition_latency`,达标 prd R2.6 三目标线;写 `bench/summary.txt` 固化基线。

### E. 质量门
- [ ] E1 `cargo build --workspace` 绿。
- [ ] E2 `cargo test --workspace` 绿(含 serde round-trip + 引擎行为 + 异步回流)。
- [ ] E3 `cargo clippy --workspace --all-targets -- -D warnings` 绿。
- [ ] E4 `cargo fmt --all -- --check` 绿。
- [ ] E5 `cargo bench --bench transition_latency` 三目标线达标,`bench/summary.txt` 固化。

## G2 IR 契约冻结评审门(v0.1 完成 → 通知用户)

- **冻结判据**:B 节完成 + 已扫父 design §3 WIT 形状对比,C 节引擎落了不变量;向后续版本保证:此 IR shape 不可破坏性改动(加字段配 `#[serde(default)]` 兼容除外)。
- **冻结产出**:`shiroha-ir` v0.1.0 发布(本地 path 依赖,非 crates.io),后续版本 `shiroha-ir = { path = "../shiroha-ir" }` 引用;冻结点 commit 打 git tag `shiroha-ir-v0.1.0`(可选,父任务决定)。
- **冻结后回退**:若 v0.2/v0.4 发现 IR 缺字段 → 回 v0.1 改 IR shape → 通知下游同步 → 文档化此回退原因。加字段不算回退。

## 验证命令

```bash
cargo build --workspace                                 # AC7 build
cargo test -p shiroha-ir                                # AC1 单测
cargo test -p shiroha-core                              # AC4/AC5 单测
cargo test --workspace                                  # 全测
cargo clippy --workspace --all-targets -- -D warnings  # AC7 lint
cargo fmt --all -- --check                              # AC7 格式
cargo bench --bench transition_latency                  # AC6 性能
cargo tree -p shiroha-ir                                # AC1 依赖叶子校验
cargo tree -p shiroha-core                              # AC3 依赖校验
```

## 回滚点

- 若 C5 浅历史算法错乱 → 回 C4 RTC 步,补单测再断点。
- 若 D5 bench 不达标 → 优先优化 `TransitionCache` 与 `CompiledState.ancestors` 查找;不放宽 prd 目标线(prd R2.6 数值给 2-5× 余量,应难达标但若不达标须复盘设计)。
- 若 IR shape 漏字段被 C 节引擎实现或 D 节测试发现缺 → 回 B 节加字段(加法兼容,不破坏 G2 已冻结则 OK;若 v0.1 内即多次反复 shape,推倒重做)。

## 进 v0.2 前置

- G2 IR 契约冻结评审通过。
- `bench/summary.txt` 提交。
- 通知父任务:此 v0.1 完成可进入归档流程;创建 v0.2 child(`task.py create "v0.2 WASM 单机运行: adapter+host-func+runner" --slug v02-wasm-runner --parent .trellis/tasks/06-25-shiroha-arch`)。