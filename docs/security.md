# 安全

## 节点认证

分布式模式下 Node 连接 Controller 需要身份验证：

| 方案       | 说明                                      | 阶段            |
| ---------- | ----------------------------------------- | --------------- |
| Join Token | Node 启动时携带 Controller 预生成的 token | Phase 2（初期） |
| mTLS       | 双向证书认证，gRPC 原生支持               | Phase 3         |

**Join Token 流程：** 管理员通过 sctl 生成带 TTL 的 token → 将 token 配置到 Node 启动参数 → Node 连接 Controller 时携带 token → Controller 验证有效性并注册 Node → 后续通信使用 session token。

## WASM 沙箱

WASM 本身提供执行隔离（详见 [WASM 权限系统](wasm-design.md#权限系统)），是安全体系的第二道防线。
