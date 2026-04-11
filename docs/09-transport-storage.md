# 传输层与持久化

## 传输层

抽象 Transport trait，初期实现 gRPC，预留接口供未来扩展：

- gRPC（初期实现）
- QUIC（预留）
- 消息队列（预留）

## 持久化

抽象 Storage trait，初期使用嵌入式存储，持久化内容：

- 状态机实例状态
- 事件日志
- WASM 模块 Registry
