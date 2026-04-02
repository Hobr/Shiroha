# Shiroha

> 由 WebAssembly 驱动的分布式状态机任务编排框架

目前处于早期设计与开发阶段。

## 开发

```bash
# Rust环境
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
cargo install just cargo-binstall
rustup target add wasm32-wasip2

# 构建
just build

# 开发
sudo apt update && sudo apt upgrade -y
sudo apt install -y protobuf-compiler libprotobuf-dev pre-commit
just install-dev
just fmt
just doc

# 更新
just update

# 发布
just release
```
