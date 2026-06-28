//! Historical runtime wiring-test compatibility markers.
//!
//! The runtime is no longer a skeleton layer. Keep these exports only for older
//! local tests and status probes that assert early crate wiring.

/// Describes a historical runtime planning marker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeTodo {
    area: &'static str,
}

impl RuntimeTodo {
    /// Creates a new compatibility marker.
    #[must_use]
    pub const fn new(area: &'static str) -> Self {
        Self { area }
    }

    /// Returns the planned area name.
    #[must_use]
    pub const fn area(&self) -> &'static str {
        self.area
    }
}

/// Stable status string used by early wiring tests.
#[must_use]
pub const fn runtime_skeleton_status() -> &'static str {
    "runtime-skeleton"
}
