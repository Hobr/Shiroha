# Shiroha

> 由WebAssembly驱动的分布式状态机任务编排框架

## 概念

- Controller 控制器: 负责Flow的创建、调度与回收的全局组件
- Executor 执行器: 负责Task的执行与反馈的分布式组件

- Flow 流程: 静态状态机
- Job 作业: 执行的实例
- Execution 执行: 被执行的最小单元

## 框架

- apps
  - [ ] shirohad 服务
    - [ ] controller 控制端
    - [ ] executor 执行端

  - [ ] sctl 命令行
  - [ ] shiroha-web Web界面
  - [ ] shiroha-desktop 桌面客户端
  - [ ] shiroha-mobile 移动客户端

- crates
  - [ ] shiroha-ir 中间表示

  - [ ] shiroha-orchestrator 编排层
    - [ ] scheduler 调度器
    - [ ] dispatcher 分发器

  - [ ] shiroha-engine 执行层
  - [ ] shiroha-runtime 运行时
    - [ ] wasm WASM
    - [ ] container 容器

  - [ ] shiroha-error 错误处理
  - [ ] shiroha-config 配置
  - [ ] shiroha-logger 日志
  - [ ] shiroha-metrics 指标
  - [ ] shiroha-tracing 追踪
  - [ ] shiroha-storage 存储
  - [ ] shiroha-network 网络
  - [ ] shiroha-auth 认证

- plugins
  - [ ] wit WIT接口
  - [ ] shiroha-sdk-rs RustSDK
  - [ ] example 示例
  - preset 预置

## 阶段

### Phase1

- 基础框架
  - shirohad 服务
  - sctl 命令行

  - shiroha-ir 中间表示
  - shiroha-orchestrator 编排层

  - shiroha-engine 执行层
  - shiroha-runtime-wasm 运行时

  - shiroha-error 错误处理
  - shiroha-config 配置
  - shiroha-logger 日志
  - shiroha-storage 存储
  - shiroha-network 网络

  - wit WIT接口
  - shiroha-sdk-rs RustSDK

### Phase2

- 增强功能
  - shiroha-metrics 指标
  - shiroha-tracing 追踪
  - shiroha-runtime-cntainer 运行时

  - example 示例

### Phase3

- 用户界面
  - shiroha-web Web界面
  - shiroha-desktop 桌面客户端
  - shiroha-mobile 移动客户端

  - shiroha-auth 认证

## 开发

```bash
git clone https://github.com/Hobr/Shiroha.git
cd Shiroha

# 环境
apt install rustup just cargo-binstall

# 构建
just build

# 开发
pip install pre-commit
just install-dev
just fmt
just doc

# 更新
just update

# 发布
just release
```
