//! Historical CLI wiring-test compatibility marker.
//!
//! The CLI has real command implementations. This export remains only for older
//! local tests and status probes that assert early crate wiring.

/// Stable status string used by early CLI wiring tests.
#[must_use]
pub const fn cli_skeleton_status() -> &'static str {
    "vm-cli-skeleton"
}
