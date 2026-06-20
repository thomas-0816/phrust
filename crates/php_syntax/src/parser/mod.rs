//! Parser internals.
//!
//! The initial workspace skeleton intentionally exposes no grammar yet. Grammar
//! modules will grow behind this boundary.

pub mod core;
pub mod event;
pub mod expected;
pub mod marker;
pub mod precedence;
