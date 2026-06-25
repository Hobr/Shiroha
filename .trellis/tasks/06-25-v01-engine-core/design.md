# v0.1 引擎内核 设计(design.md)

> v0.1 child task `.trellis/tasks/06-25-v01-engine-core` 技术设计。父任务产品决策见父 `prd.md`(D1/D3/D5/D7),父技术设计见父 `design.md`(§2 IR / §4 动作执行 / §6 crate)。术语以 `glossary.md` 为权威。本文件给出 IR 完整字段、引擎接口签名、内部表示、RTC 算法、转换路径缓存、动作 future trait、mock 设计、crate 内结构。**G2 IR 契约冻结点**:字段 shape 一次定对,下游 v0.2+ 不改 IR shape(加法兼容除外)。

## 1. Crate 边界与依赖

```
shiroha-ir      ── 零上游 (仅 serde)
shiroha-core    ── 仅依赖 shiroha-ir
benches/(core)  ── 依赖 shiroha-core + criterion
```

`cargo tree -p shiroha-ir` 只出现 `serde`;`cargo tree -p shiroha-core` 只出现 `shiroha-ir` + `serde`(无 `tokio`/`wasmtime`/`tonic`/`opentelemetry`/`futures`)。AC1/AC3 据此验收。

## 2. SmIr 完整字段(`shiroha-ir`)

```rust
use std::collections::BTreeMap;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SmIr {
    pub name: String,
    pub root: StateRef,
    pub states: Vec<StateNode>,
    pub transitions: Vec<Transition>,
    pub actions: Vec<ActionDecl>,
    pub history: Vec<HistoryDecl>,
    pub capabilities: Vec<CapabilityDecl>,
}

pub type StateRef = usize; // index into SmIr.states

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StateNode {
    pub id: String,
    pub kind: StateKind,                 // atomic / compound / final
    pub children: Vec<Region>,           // 空 = 原子态
    pub entry: Vec<ActionRef>,           // enter 动作
    pub exit: Vec<ActionRef>,            // exit 动作
    pub run: Option<ActionRef>,          // run-to-completion 动作
    pub history: Option<HistoryRef>,     // 该 compound state 的浅历史伪状态
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Region {
    pub id: String,
    pub kind: RegionKind,                // sequential / parallel
    pub initial: StateRef,               // 进入该 region 的默认子状态
    pub states: Vec<StateRef>,            // region 内子状态索引
    pub history: Option<HistoryRef>,     // region 级历史(浅;deep 预留)
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StateKind  { Atomic, Compound, Final }
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RegionKind { Sequential, Parallel }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Transition {
    pub from: Vec<StateRef>,              // source state(s) (并行跨域 join 用多源)
    pub to: Vec<StateRef>,                // target state(s) (并行跨域 fork 用多目标)
    pub event: Option<String>,           // None = 无触发器 transition
    pub guard: Option<Expr>,             // guard 表达式(见下)
    pub actions: Vec<ActionRef>,          // transition 动作
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expr {
    True,
    False,
    EventValue { key: String, op: Cmp, value: serde_json::Value },
    And(Vec<Expr>), Or(Vec<Expr>), Not(Box<Expr>),
}
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Cmp { Eq, Ne, Lt, Le, Gt, Ge }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActionDecl {
    pub name: String,
    pub r#ref: ActionRef,
    pub input: Option<Expr>,             // 动作入参表达式(可从 event payload 取)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionRef {
    WasmFunc    { export: String },
    Plugin      { plugin_id: String, method: String },
    Distributed {
        inner: Box<ActionRef>,           // = WasmFunc 或 Plugin(二选一)
        fanout: Option<u32>,
        target: Option<TargetSpec>,
        aggregate: AggregateRef,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CapabilityDecl {
    pub interface: String,              // "wasi:filesystem" / "shiroha:shell" ...
    pub functions: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HistoryDecl {
    pub owner: StateRef,                  // 所属 compound state / region
    pub depth: HistoryDepth,              // Shallow 默认;Deep 预留未实现
}
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case", default = "HistoryDepth::Shallow")]
pub enum HistoryDepth { #[default] Shallow, Deep }
pub type HistoryRef = usize;             // 在 SmIr.history 中的索引

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TargetSpec { Any, Pool(String), Label(String, String), Explicit(Vec<String>) }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AggregateRef {
    Builtin   { strategy: BuiltinAggregate },
    WasmFunc  { export: String },
    Plugin    { plugin_id: String, method: String },
}
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BuiltinAggregate { All, Any, Quorum(u32), FirstSuccess }
```

`BTreeMap` 不需要进 IR,纯 Vec+index。`serde_json::Value` 仅用于 guard 比较值(随 event payload 传来);`shiroha-ir` 依赖 `serde` + `serde_json`(允许:serde_json 是纯 serde 大码,不引入 runtime)。绝对依赖清单:`serde`、`serde_json`。

G2 冻结说明:此 IR shape 定后,下游 v0.2 / v0.4 / v0.7 / v0.10 只允许**加字段(配 `#[serde(default)]`)**,不允许 rename/删除/change tag。

## 3. 引擎接口(`shiroha-core`)

```rust
pub struct Engine {
    ir: CompiledIr,
}

pub struct TaskInstance {
    active: ActiveConfig,           // 当前 active state set
    inflight: InflightTable,        // task_id × action → ActionToken
    completed: VecDeque<Completion>, // 完成事件队列
    events: VecDeque<Event>,        // 待处理输入事件队列
    history: HistoryStore,          // 浅历史 slot
}

pub type ActionToken = usize;       // 引擎侧给 inflight 动作的 token
pub type TaskId = u64;

pub struct Event {
    pub name: String,
    pub payload: serde_json::Value,
}

pub enum Completion {
    Done   { action: String, token: ActionToken, result: Vec<u8> },
    Error  { action: String, token: ActionToken, error: String },
}

impl Engine {
    pub fn new(ir: SmIr) -> Result<Self, CompileError>;   // 编译 IR → CompiledIr,校验 + 建缓存
    pub fn create_instance(&self) -> TaskInstance;         // 从 root 起始 active set
    pub fn submit_event(&self, inst: &mut TaskInstance, ev: Event);
    pub fn poll_advance(&self, inst: &mut TaskInstance)
        -> PollOutcome;                                    // RTC 推进(synchronous),触发异步动作 future
    pub fn take_completions(&self, inst: &mut TaskInstance) -> Vec<Completion>;
    pub fn drive_completion(&self, inst: &mut TaskInstance, c: Completion)
        -> PollOutcome;                                    // 把外驱动作完成回填,继续推进
}

pub enum PollOutcome {
    Idle,                       // 无可处理事件也无 inflight 待回流
    TriggeredActions(Vec<TaskAction>), // RTC 已完成结构转换,触发这些动作(异步)
    Done,                       // 完成了结构转换无新动作,可继续 poll
}
pub struct TaskAction {
    pub action: String,
    pub token: ActionToken,
    pub input: Vec<u8>,        // ActionDecl.input 求值后的字节(payload 是 JSON)
}
```

- `submit_event` 只入队,**不立刻执行**(让 runtime 控制节奏)。
- `poll_advance` 是**同步 RTC 段**:处理下一个事件/完成事件 → 选转换 → 算 LCA → 退/进动作入动作序列 → active set 更新。返回 `TriggeredActions(Vec<TaskAction>`)时,RTC 已完成、动作 future 由上层 runtime 驱动并最终经 `drive_completion` 回填。
- `poll_advance` 不 future-aware;动作 future 是上层 runtime 用 `Box::pin(async move { ... })` 构造,完成时调 `drive_completion`。`shiroha-core` 因此**不依赖 `futures` crate**——动作 future 由上层 runtime 持有,core 只看 token 与完成 outcome。
- AC8:此接口足够 v0.2 wasm runner 用「外置 Runtiime」驱动动作 future + 完成回填;足够 v0.3 controller 在 tokio runtime 上挂多实例并发。

## 4. 内部表示(CompiledIr)

```rust
struct CompiledIr {
    states: Vec<CompiledState>,
    transitions: Vec<CompiledTransition>,
    parent: Vec<Option<StateRef>>,      // state -> 父 state(扁平化必备,算 LCA)
    region_states: Vec<Vec<StateRef>>,  // region -> states(并行/串行)
    cache: Mutex<TransitionCache>,       // 多实例共享路径缓存
}
struct CompiledState {
    node: StateNode,
    regions: Vec<Region>,
    is_compound: bool,
    is_final: bool,
    ancestors: Vec<StateRef>,           // root→self 路径(LCA 用,做 O(depth))
}
struct CompiledTransition {
    from: Vec<StateRef>, to: Vec<StateRef>,
    event: Option<String>, guard: Option<Expr>, actions: Vec<ActionRef>,
    exits: Vec<StateRef>,               // 预计算 exit 序列
    enters: Vec<StateRef>,              // 预计算 enter 序列
    lca: StateRef,                       // 预计算 LCA
}
```

预计算 `exits`/`enters`/`lca` 在编译时刻完成(RTC 时直接走表),`CompiledTransition` 一次定型不变;转换缓存只针对**事件→候选转换集合**做 fast lookup(同一 from_state + event 的 transition 列表)。多实例共享 `Engine`(读多写少),`task instance` 局部 `TaskInstance`。

## 5. RTC 算法(同步推进)

`poll_advance` 单步:
1. 取下一个事件/完成事件(`inst.events.pop_front()` 或 `inst.completed.pop_front()`)。
2. 在当前 active set 上求 transitions:若有 `event` 匹配且 guard 通过 → 选中(优先级为源:最内层 active state 优先)。
3. 执行 exit 动作(active set 中需要离开的状态,按 LCA 顺序逆序 exit)→ run transition actions → enter 新状态(按 LCA 顺序顺序 enter),每步抽出 `ActionRef` 序列入 `TriggeredActions`。
4. 更新 active set(并行 region 内全部 active)。
5. 浅历史:离开 compound state 时记下「上次 active 直接子状态」;入口走 history pseudo 时不走 default initial。
6. 触发 entry actions 的 invoke 语义:如果 entry 完成后可能驱动下一转换(xstate-invoke 风格),entry 完成事件入 `completed` 队列,下次 poll 推进。

RTC 段是非阻塞纯函数,被 bench 测的正是此段。

## 6. 转换路径缓存

`TransitionCache: HashMap<(StateRef, Option<String>), Vec<IdxOf<CompiledTransition>>>`——键为 `(当前 active state, 事件名)`。多实例共享 `Engine`,初始 `Mutex` 单写一次命中即查表;缓存命中率在深嵌套 fixture 中应 ≥ 80%。

eviction:none,缓存键空间是 O(states × events),编译期可能数固定。无需 LRU。

## 7. 动作 future trait & mock 设计

`ActionRef` 有两种:`WasmFunc` / `Plugin`(v0.1 都不接真实执行,设计 mock)。设计一个 trait:

```rust
pub trait ActionRuntime: Send + Sync {
    /// 由 runtime 实现:按 ActionRef 驱动动作,完成时回填 Completion。
    /// v0.1 测试用 MockActionRuntime(由测试 fixture 直接 yield Done/Error)。
    fn dispatch(&self, action: &TaskAction, sink: &(dyn CompletionSink + Send + Sync));
}
pub trait CompletionSink {
    fn complete(&self, c: Completion);
}
```

- core **不持有** `ActionRuntime`,它由上层注入;`poll_advance` 返回 `TriggeredActions`,runtime 拿去调 `action_runtime.dispatch(...)`。
- v0.1 测试用 `MockActionRuntime`:按 fixture 预设的 outcome(立即 Done/Error 或延时)回填。
- v0.2 真实 `WasmActionRuntime` 会包 wasmtime + tokio,调 `instance.get_func(name).typed().call_async(...)`,完成时 `sink.complete(...)`。
- v0.4 真实 `PluginActionRuntime` 解析 `{plugin_id, method}` 调插件。
- `Distributed` 包装的 action_ref 在 v0.1 只做 IR 定义,引擎未拆分发(v0.5 才走 scheduler);Runtime 可拒绝 `Distributed` ref,v0.1 不测分布式。

## 8. 测试 fixture

- IR fixtures:数个 `SmIr` 文字构造(无 serde-width 来,纯构造)用 → 引擎编译 → instance → 推进 → 验证 active set + 完成事件序列。
- 复用 SCXML 语义测试章法:compound entry/exit/transition/run,transitions 选择优先级,orthogonal region 并行,history 浅恢复,guard true/false。
- mock 动作:`MockActionRuntime` 按 fixture 注入预期 outcome,验证完成事件回流驱动下一转换(AC5)。
- bench fixtures 三类(见 prd R2.6);criterion 默认统计,在 benches/ 下定义。

## 9. 风险与缓解

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| IR shape 一旦遗漏字段,v0.2 / v0.4 / v0.10 须改 shape → 破坏 G2 冻结 | 阻断下游 | design.md §2 列全字段 review;先扫父 design §3 WIT 形状对比,确保 `CapabilityDecl` + `ActionRef::Plugin` 不遗漏;`#[serde(default)]` 配置凡可空字段。 |
| active set 表示选错导致 LCA 算错 | 转换错位 | `CompiledState.ancestors` 预存 + LCA = 两条 ancestor 路径最低共同节点;单测覆盖嵌套+并行+跨域 join。 |
| RTC 与异步动作责任模糊 | 设计回退 | design.md §3、§5 边界:core 只出 `TriggeredActions` + 接 `drive_completion`,不持 future;runtime 驱动。AC8 守护接口够用。 |
| 性能 bench 数值过严达不到 | v0.1 闭环受阻 | bench 目标给 2-5× 余量(prd R2.6),先固基线,再压缩;如某 fixture 不达标,优先优化缓存/active set 表而不放宽目标。 |
| `serde_json::Value` 进 IR 让 ir 非纯 serde | 审美 + 依赖边界争议 | `serde_json` 是 serde 生态叶子无运行时,允许;文档化此决策。 |
| Mock 动作聚合 overdesign | 拖慢 v0.1 | Mock 设计最小化:fixture 预设 (action_name → outcome),CompletionSink 立即回填;不模拟 wasm plugin 加载等未实现路径。 |

## 10. 待实现期细化(不阻塞 G1 review)

- Transition 优先级规则(同事件下多源转换选择,按 SCXML 语义:最深的 source state)。
- `final` state 语义(父 compound state 完成,触发其 exit)。
- 并行 region 全部到 final → 父 compound 自动 completion。
- 子任务实现时遇未明语义补单测,不改 G2 IR shape 不破坏父 design。