# 实施阶段

| 阶段 | 内容 | 里程碑 |
|------|------|--------|
| P0 | WIT 契约 + model + guest/sdk | 能编译出示例 WASM 模块 |
| P1 | wasm-engine（含权限守卫） | 加载 WASM、提取定义、执行 Action |
| P2 | fsm（Effect 模式） | 纯逻辑状态流转测试通过 |
| P3 | dispatch(local) + storage | 单进程 Standalone 模式跑通全流程 |
| P4 | transport(gRPC) + dispatch(remote) | 远程分发跑通 |
| P5 | server + cli | 完整系统可交互 |
