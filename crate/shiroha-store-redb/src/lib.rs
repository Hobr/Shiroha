//! Redb 持久化存储后端入口。
//!
//! 对外只暴露 `store` 模块，避免调用方依赖 redb 细节类型。
pub mod store;
