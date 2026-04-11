# 状态机版本化

- 以 `(machine_name, wasm_hash)` 唯一标识一个状态机版本
- 已运行的实例绑定到部署时的 wasm_hash
- 新部署自动使用新版本，不影响存量实例
