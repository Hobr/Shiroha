//! Shiroha 核心类型库
//!
//! 定义框架中所有共享的数据结构和 trait，包括：
//! - 状态机定义（Flow）及其组成部分
//! - Job 运行实例与执行结果
//! - 事件溯源记录
//! - 存储与传输层的抽象 trait

// 框架级错误类型
pub mod error;
// 事件溯源记录
pub mod event;
// 状态机定义（Flow）
pub mod flow;
// Job 运行实例与执行结果类型
pub mod job;
// 存储层抽象
pub mod storage;
// 传输层抽象
pub mod transport;
