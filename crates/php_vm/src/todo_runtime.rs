//! Historical VM wiring-test compatibility markers.
//!
//! The VM is no longer a skeleton layer. Keep these exports only for older local
//! tests and status probes that assert early crate wiring.

/// Describes a historical VM planning marker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmTodo {
    area: &'static str,
}

impl VmTodo {
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
pub const fn vm_skeleton_status() -> &'static str {
    "vm-skeleton"
}
