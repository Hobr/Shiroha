# WIT 接口设计

## 设计思路

将 Guest 导出拆为两个独立接口，Host 能力按职责隔离，使 Controller 和 Node 各取所需。

## Guest 导出

- **definition** — 返回状态机定义和能力需求清单。Controller 加载时调用，不需要注入任何 Host 能力。
- **action** — 执行具体的 Action/Callback。Node 执行时调用，需要注入 Host 能力。

## Host 提供

每项 Host 能力独立为一个 interface，按 WASM 模块声明的需求选择性注入：

- **http** — HTTP 请求
- **kv** — 键值存储
- **log** — 结构化日志

## World 组合

- **machine-definition** — 仅导出 definition，用于 Controller 侧解析
- **machine-action** — 导入 host 能力 + 导出 action，用于 Node 侧执行
- **machine-full** — 两者合并，单个 WASM 模块同时实现

## 文件结构

```
wit/shiroha/
├── types.wit        共享类型
├── definition.wit   Guest: 状态机定义
├── action.wit       Guest: Action 执行
├── http.wit         Host: HTTP 能力
├── kv.wit           Host: KV 能力
├── log.wit          Host: 日志能力
└── world.wit        World 组合
```
