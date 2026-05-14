# 待对齐的设计决策

本文档汇总尚未敲定的设计点。每条决策一旦确认,应从本文档移除并迁移到相应模块文档。

**已敲定并迁移**(2026-05-15):

- Q1 → Flow 抽象主控层独有(`storage.md`、`core-model.md`、`control-plane.md`)
- Q2 → Action 同步性由 `ActionRef.WaitingMode` 逐条声明(`core-model.md`、`engine.md`)
- Q3 → 节点注册 MVP 静态、后续混合(`transport.md`)
- Q4 → WASM 组件按需 pull(`worker.md`、`data-flow.md`)
- Q5 → Aggregator 提前返回后取消 + 后台兜底(`dispatch.md`、`transport.md`)
- Q6 → Blocking 在途 Action 主控重启时向节点查询结果缓存(`worker.md`、`engine.md`、`data-flow.md`)
- Q7 → 能力清单确定:log / clock / net.http(GET+POST)/ kv(per-Job)/ fs.readonly(白名单)/ rand(仅 CSPRNG)(`wit-interfaces.md`)
- Q8 → Flow 删除默认拒绝、`--force` 取消相关 Job 后再删(`storage.md`、`engine.md`、`control-plane.md`)
- Q9 → Event 与 Job 生命绑定,Job 终态 N 天后清理(`storage.md`)

**目前无未决项。** 后续新决策请按底部"决策流程"添加。

---

## 决策流程

- 新决策点出现时,**先在本文档新增条目**;不要把"待定"散落到各处
- 决策落地后,从本文档删除对应条目,把结论写入相应模块文档
- 重大决策(影响 ≥ 2 个 crate)需要在 PR 描述里点名本文档对应章节
- 决策应当显式说明默认倾向与触发选择变化的条件,避免日后"原因丢失"
