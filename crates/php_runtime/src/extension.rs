//! Inward-facing contract implemented by statically selected PHP extensions.

use crate::builtins::BuiltinEntry;
use std::any::Any;
use std::any::TypeId;
use std::mem::size_of;

/// Host service requested by an extension implementation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ExtensionCapability {
    Filesystem,
    Network,
    Environment,
    Clock,
    ProcessSharedState,
}

/// Class-like metadata contributed by an extension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtensionType {
    pub name: &'static str,
}

/// Constant metadata contributed by an extension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExtensionConstant {
    pub name: &'static str,
}

/// Factory for state allocated only when its extension is selected.
#[derive(Clone, Copy)]
pub struct ExtensionStateFactory {
    type_name: &'static str,
    create: fn() -> Box<dyn Any>,
    type_id: fn() -> TypeId,
    payload_bytes: usize,
}

fn extension_state_type_id<T: Any>() -> TypeId {
    TypeId::of::<T>()
}

impl ExtensionStateFactory {
    /// Declares one concrete request-state type for registration-time layout assembly.
    #[must_use]
    pub const fn of<T: Any>(type_name: &'static str, create: fn() -> Box<dyn Any>) -> Self {
        Self {
            type_name,
            create,
            type_id: extension_state_type_id::<T>,
            payload_bytes: size_of::<T>(),
        }
    }

    #[must_use]
    pub const fn type_name(self) -> &'static str {
        self.type_name
    }

    #[must_use]
    pub fn type_id(self) -> TypeId {
        (self.type_id)()
    }

    #[must_use]
    pub const fn payload_bytes(self) -> usize {
        self.payload_bytes
    }

    #[must_use]
    pub const fn create_fn(self) -> fn() -> Box<dyn Any> {
        self.create
    }
}

impl std::fmt::Debug for ExtensionStateFactory {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ExtensionStateFactory")
            .field("type_name", &self.type_name)
            .finish_non_exhaustive()
    }
}

/// Static metadata and entry points for one built-in extension.
#[derive(Clone, Copy, Debug)]
pub struct ExtensionDescriptor {
    pub name: &'static str,
    pub version: &'static str,
    pub dependencies: &'static [&'static str],
    pub functions: &'static [BuiltinEntry],
    pub classes: &'static [ExtensionType],
    pub constants: &'static [ExtensionConstant],
    pub request_state: Option<ExtensionStateFactory>,
    pub capabilities: &'static [ExtensionCapability],
    pub initialize: Option<fn() -> Result<(), &'static str>>,
    pub shutdown: Option<fn()>,
}

/// Rust-native contract for extensions selected by an integration facade.
pub trait ExtensionModule: Sync {
    fn descriptor(&self) -> &'static ExtensionDescriptor;
}
