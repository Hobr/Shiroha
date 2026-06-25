# v0.1 引擎内核: SmIr + core statechart

> Shiroha 父任务 `.trellis/tasks/06-25-shiroha-arch` 的 v0.1 child task。产品决策见父 `prd.md`(D1/D3/D5/D7),技术设计见父 `design.md`(§2 IR / §4 动作执行),术语以 `glossary.md` 为权威。本文件只放需求与验收;技术细节进 `design.md`(待写)。

## Goal

交付 Shiroha 框架的引擎内核:统一 IR 契约 `SmIr`(serde-only 叶子,一次定对含 `CapabilityDecl` + `ActionRef::Plugin{plugin_id, method}` + 聚合引用)+ 纯逻辑的层级+并行 statechart 引擎(结构转换同步 RTC、异步动作 future、完成事件队列、转换路径缓存),不依赖 tokio/wasmtime/tonic,用 mock 动作单测。完成即满足父任务 **G2 IR 契约冻结点**。

## Background

- v0.1 = 父 implement.md 版本路线第 1 个版本,依赖 = 无(自起)。
- 产出 crate:`shiroha-ir`、`shiroha-core`。后续 v0.2+(wasm adapter/worker)、v0.3(控制器/持久化)、v0.7(文本 adapter)、v0.10(完整授权)全部依赖此处的 IR 契约,故 IR 必须**一次定对**。
- 父任务的「核心不变量」:`shiroha-ir` = serde-only 叶子(零上游);`shiroha-core` 仅依赖 `shiroha-ir`(纯逻辑,无 runtime/wasm/network,可独立单测)。

## Confirmed Facts(来自父产物,无需再问)

- **状态机语义**(D1):层级 + 并行/正交区域 + guard + entry/exit/run action + **浅历史**;扁平化内部表示 + 转换路径缓存保性能;**深历史为可选扩展**(是否纳入 v0.1 见 Open Questions)。
- **动作执行语义**(D3):结构转换步**同步 RTC 原子**完成(选转换 / 算 LCA / 定 exit·run·enter 顺序);动作**异步**;完成抛 `done.<action>` / 失败 `error.<action>` 事件入实例事件队列驱动后续转换(xstate-invoke 风格)。
- **实例/任务模型**(D5):多实例引擎,task = 一个状态机实例,状态默认驻内存。
- **IR 结构**(父 design §2):`SmIr{name, root, states, transitions, actions, history, capabilities}`;`ActionRef{WasmFunc, Plugin, Distributed{...}}`;`AggregateRef{Builtin, WasmFunc, Plugin}`;`CapabilityDecl{interface, functions}`;`TargetSpec{Any,Pool,Label,Explicit}`;`BuiltinAggregate{All,Any,Quorum,FirstSuccess}`。
- **动作执行内容二选一**(R3.1):`wasm func`(机器自身组件)或 `plugin`(`{plugin_id, method}`)。v0.1 **只在 IR 层定义这两种 ref**,实际执行用 mock(v0.2 才接 wasm,v0.4 才接 plugin)。
- **capability 与 plugin 正交**(D7/R3.5):v0.1 仅在 IR 定对 `CapabilityDecl` 契约,不实现运行时授权(v0.10)。
- **core 无 runtime 依赖**(父 design §4):core 用 `BoxFuture` 表示动作 future,不 `tokio::spawn`;暴露「提交事件 / poll 推进 / 取完成事件」接口,由上层 runtime 驱动并回填结果。这让 core 可独立单测。
- **历史策略(已决)**:v0.1 **只实现浅历史**;深历史为可选扩展,但 `HistoryDecl` 的 shape 在 v0.1 即预留——`depth: HistoryDepth` 枚举(`Shallow` 默认 / `Deep` 预留,serde `#[serde(default)]`),未来加 `Deep` 语义是非破坏兼容(不改 shape,仅引擎侧补恢复路径),无需回归 IR 契约。

## Requirements

### R1 IR 契约(`shiroha-ir`)
- R1.1 **`SmIr` 一次定对**(G2 冻结点):所有字段按父 design §2 完成;serde-derived(`Serialize`+`Deserialize`),`#[serde(tag, rename_all)]` 标注;无 wasmtime/tonic 依赖。
- R1.2 `ActionRef::Plugin{plugin_id, method}` 与 `CapabilityDecl{interface, functions}` 在 v0.1 即定型(即使执行侧未接),避免 v0.4/v0.10 破坏性改动。
- R1.3 `AggregateRef`(`Builtin` 4 种 + `WasmFunc`/`Plugin`)+ `TargetSpec` + `Distributed` 包装在 IR 层定义。
- R1.4 `StateNode` 编码嵌套 + 正交区域(`children: Vec<Region>`,region 内含子状态);`HistoryDecl` 含 `depth: HistoryDepth`(`Shallow` 默认 / `Deep` 预留,`#[serde(default)]`),v0.1 只实现浅历史恢复,深历史字段预留但引擎侧返回 `Unsupported` 或忽略;`Transition` 含 guard/event/source/target/action-refs。
- R1.5 TOML 友好:`SmIr` 用 `[states.<id>]` 表形式,保证 v0.7 文本 adapter 回归零成本。

### R2 引擎核心(`shiroha-core`)
- R2.1 层级 + 并行 statechart:嵌套状态、正交/并行区域、LCA 转换链、guard 求值、entry/exit/run action 触发、**浅历史恢复**(深历史字段已预留但 v0.1 不实现,引擎遇 `Deep` 按未支持处理)。
- R2.2 RTC 转换步:选转换 → 算 LCA → 定 exit·run·enter 顺序,**原子完成,不做中途突变**。
- R2.3 异步动作:转换结构定后触发动作,动作为 `BoxFuture`(or 等价 trait object),core 不阻塞等待;完成时抛 `done.<action>`/`error.<action>` 事件入实例事件队列。
- R2.4 引擎接口(`submit_event` / `poll_advance` / `take_completed` 形态,具体签名进 design),runtime 负责调度与回填。
- R2.5 in-flight 动作表 + 完成事件队列管理;入口动作 invoke 语义(完成事件可驱动下一转换)。
- R2.6 **转换路径缓存**:扁平化内部表示 + 缓存常用转换路径,per-event 处理开销用 `criterion` bench 测**纯同步 RTC 段**(事件提交 → 转换决策 → active state configuration 更新,不含动作 future 驱动;机器暖机后测稳态),目标线(暖机 / 建缓存 / `cargo bench --bench transition_latency`):
  | Fixture | 规模 | 目标 |
  | --- | --- | --- |
  | 扁平 FSM 基线 | 50 状态、~200 转换、无嵌套 | **≤ 100 ns / event** |
  | 中等嵌套 | 3 层 × 5 并行区域 × ~100 转换 | **≤ 300 ns / event**(LCA 计算占大头) |
  | 深嵌套 + 路径缓存命中 | 5 层 × 3 并行 + 50 历史节点 | **≤ 500 ns / event**,缓存命中率 ≥ 80% |
  数值为推荐基线,实际可参考 Rust 生态(`statig`/`smlang` 扁平 FSM 在 50ns 量级)+ Shiroha 层级/并行/LCA 固定开销(给 2–5× 余量)调校;AC6 达标后在 `bench/summary.txt` 固化基线。
- R2.7 `shiroha-core` 仅依赖 `shiroha-ir`;无 `tokio`/`wasmtime`/`tonic`/`opentelemetry` 依赖。
- R2.8 多实例:一个引擎进程内可承载多个独立 task 实例,每实例独立事件队列;状态默认驻内存(持久化在 v0.3)。

## Acceptance Criteria

- [ ] AC1 `shiroha-ir` 作为 serde-only 叶子发布:仅依赖 serde,无 wasmtime/tokio/tonic;`SmIr` 全字段定义完成且通过 `cargo test -p shiroha-ir`(serde round-trip fixtures)。
- [ ] AC2 IR 契约一次定对:`ActionRef::Plugin{plugin_id, method}` + `CapabilityDecl{interface, functions}` + `AggregateRef` + `TargetSpec` + `Distributed` 全部就位,v0.2/v0.4/v0.7/v0.10 据此开发无需改动 IR 字段(语义增加而非 shape 变更除外)。
- [ ] AC3 `shiroha-core` 仅依赖 `shiroha-ir`(无 runtime/wasm/network),`cargo tree -p shiroha-core` 验证。
- [ ] AC4 层级/并行 statechart 引擎行为正确:嵌套状态进入/退出、正交区域并行激活、LCA 转换链 exit·run·enter 顺序、guard 拦截、浅历史恢复——单测覆盖。
- [ ] AC5 RTC 原子转换 + 异步动作 + 完成事件回流:mock 动作 future 被驱动完成 → `done.<action>`/`error.<action>` 入队 → 驱动后续转换,单测覆盖(xstate-invoke 风格)。
- [ ] AC6 per-event 性能基准建立 + 三 fixture 达标:扁平 FSM ≤100ns、中等嵌套 ≤300ns、深嵌套+缓存命中 ≤500ns 且命中率 ≥80%(criterion `transition_latency` bench,暖机稳态,纯同步 RTC 段);基线固化为 `bench/summary.txt`。
- [ ] AC7 `cargo build --workspace`(v0.1 阶段 = ir+core)+ `cargo test --workspace`+ `cargo clippy -- -D warnings` + `cargo fmt --check` 全绿。
- [ ] AC8 引擎接口(`submit_event`/`poll_advance`/`take_completed`)足够支撑 v0.2 wasm runner 与 v0.3 控制器上层驱动,无需回改 core(即上层可基于未来 BoxFuture 接口直接 wiring)。

## Out of Scope(v0.1 不做)

- WASM CM adapter / wasm 动作执行 / 最小 host-func 通道(v0.2)。
- plugin 加载 / semver 协商 / 沙箱(v0.4)。
- 控制器 / 多实例托管 API / 持久化 / 崩溃恢复(v0.3)。
- 完整 capability 运行时授权(v0.10)。
- 文本 adapter(v0.7)。
- 分布调度器 / worker(v0.5)。
- OTel exporter / 埋点接入(v0.3/v0.6)。

## References

- 父 `prd.md`(D1/D3/D5/D7、R1/R2.1–2.3/R3.5、版本路线 v0.1 行)
- 父 `design.md`(§2 IR、§4 动作执行、§6 crate 布局、§12 待细化点)
- 父 `implement.md`(v0.1 行、G2 冻结点)
- 父 `glossary.md`(SmIr / statechart / RTC / action ref / capability / plugin 等术语)