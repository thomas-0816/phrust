//! Shared CLI entrypoints for the VM package.
//!
//! The CLI crate owns command-line process integration, argument parsing,
//! report/debug commands, disk bytecode-cache policy, and compatibility binary
//! entrypoints. Normal PHP execution delegates compile/execute orchestration to
//! `php_executor`; specialized inspection commands may still call frontend,
//! optimizer, bytecode, or VM APIs directly when they need internal metadata.

pub mod engine;
pub mod php_cli;
