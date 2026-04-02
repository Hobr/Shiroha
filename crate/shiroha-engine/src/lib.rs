//! Shiroha 状态机引擎
//!
//! 负责状态机的驱动、Job 生命周期管理、定时器、调度和 Flow 验证。
//! 不直接依赖 WASM 运行时——通过 trait 抽象与上层解耦。

// 状态机驱动器
pub mod engine;
// Job 生命周期管理
pub mod job;
// 调度策略
pub mod scheduler;
// 定时器轮
pub mod timer;
// Flow 静态验证器
pub mod validator;
