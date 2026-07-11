//! Inward-facing contract implemented by statically selected PHP extensions.

use crate::builtins::BuiltinEntry;
use std::any::Any;

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
    pub type_name: &'static str,
    pub create: fn() -> Box<dyn Any>,
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
