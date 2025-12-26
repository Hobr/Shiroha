# Shiroha

> 由WebAssembly驱动的分布式任务编排执行框架

## 概念

- Controller 控制器: 负责Flow的创建、调度与回收的全局组件
- Executor 执行器: 负责Task的执行与反馈的分布式组件

- Flow 流程: 描述Job如何被创建、拆分、调度与回收的静态或半静态逻辑
- Job 作业: Flow的一次具体执行实例, 具有完整生命周期与全局状态
- Task 任务: Job中被调度到某个Executor的最小无状态执行单元

## 框架

- apps
  - [ ] shirohad 服务
    - [ ] controller 控制
      - [ ] api API接口
    - [ ] executor 执行

  - [ ] sctl 命令行
  - [ ] shiroha-web Web界面
  - [ ] shiroha-desktop 桌面客户端
  - [ ] shiroha-mobile 移动客户端

- crates
  - [ ] shiroha-ir 中间表示

  - [ ] shiroha-core 核心
  - [ ] shiroha-scheduler 调度器
  - [ ] shiroha-dispatcher 分发器

  - [ ] shiroha-engine 引擎
  - [ ] shiroha-runtime 运行时

  - [ ] shiroha-config 配置
  - [ ] shiroha-storage 存储
  - [ ] shiroha-network 网络
  - [ ] shiroha-logger 日志
  - [ ] shiroha-error 错误处理

- plugins
  - [ ] wit WIT接口
  - [ ] shiroha-sdk-rs RustSDK
  - [ ] example 示例
  - preset 预置

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
