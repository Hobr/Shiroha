# 模块划分与依赖

> 文档定位：本文描述目标模块边界与期望依赖关系，不代表这些 crate / 目录已经全部在仓库中落地。

## Host 侧（编译目标：native）

```
crates/
├── model           核心数据模型，含定义/部署/task/结果等纯类型，零外部依赖
├── engine          WASM 运行时封装 + WIT 绑定 + 能力注入 + 部署校验
├── machine         状态机引擎，纯逻辑，Effect 模式，不感知外部世界
├── dispatch        task 规划、分发、重试、取消、聚合，含本地和远程两条路径
├── transport       控制面/任务面传输抽象 + gRPC 实现 + Protobuf 定义
└── storage         持久化抽象，保存实例、task、部署和模块索引

app/
├── shirohad        统一入口，按配置激活 Controller / Node / Standalone
└── sctl            CLI 客户端，通过管理接口与 shirohad 交互
```

## Guest 侧（编译目标：wasm32-wasip2）

```
guest/
├── sdk             Guest 开发者依赖的 SDK，封装 WIT 绑定为友好 API
└── sdk-macros      过程宏（编译目标 native），简化状态机和 Action 定义
```

## 接口定义

```
wit/
└── shiroha/
    ├── types.wit        共享类型（能力枚举、内建分发/聚合策略、状态机定义结构）
    ├── definition.wit   Guest 导出：状态机定义 + 能力声明
    ├── action.wit       Guest 导出：Action/Callback 执行入口
    ├── http.wit         Host 提供：HTTP 能力
    ├── kv.wit           Host 提供：KV 存储能力
    ├── log.wit          Host 提供：日志能力
    └── world.wit        World 组合（definition / action / full）
```

## 依赖关系

```
model (零依赖)
  ↑
  ├── machine
  ├── engine
  ├── dispatch
  ├── transport
  └── storage
        ↑
     shirohad (组装层，依赖以上全部)
        ↑
     sctl (管理接口客户端，依赖 transport 客户端部分)

── Guest 侧独立编译链 ──

sdk-macros (proc_macro, native)
  ↑
sdk (wasm32-wasip2, 依赖 WIT 绑定)
  ↑
用户 WASM 模块
```
