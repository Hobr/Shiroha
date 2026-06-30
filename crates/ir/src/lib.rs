//! # shiroha-ir
//!
//! Internal Representation (IR) for state machine definitions.
//! This crate is the unified intermediate representation shared by all adapters.

mod error;
mod types;

pub use error::*;
pub use types::*;

#[cfg(test)]
mod tests;
