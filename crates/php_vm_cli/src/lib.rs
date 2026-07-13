//! Shared CLI entrypoints for the VM package.
//!
//! The CLI crate owns command-line process integration and compatibility binary
//! entrypoints. PHP execution delegates to the mandatory-native coordinator;
//! inspection is limited to backend-neutral compile and IR output.

pub mod engine;
pub mod php_cli;
