//! Executor-owned include compiler port consumed by the VM.

use super::resolver::IncludeLoader;
use super::source::{IncludeDependency, ValidatedIncludeSource};
use crate::compiled_unit::CompiledUnit;
use crate::error::VmError;

/// Opaque identity for one compiler configuration and dependency map.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct IncludeCompilerFingerprint(String);

impl IncludeCompilerFingerprint {
    /// Creates a stable fingerprint. Concrete compilers own its contents.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// Result of compiling one validated include source.
#[derive(Debug)]
pub struct CompiledInclude {
    /// Executable unit consumed by the VM.
    pub unit: CompiledUnit,
    /// Exact dependency identities opened while building the unit.
    pub dependencies: Vec<IncludeDependency>,
}

/// Compiler port consumed by the VM include and eval paths.
///
/// The concrete implementation belongs to the executor layer. This port lives
/// beside its VM consumer so `php_executor -> php_vm` remains a one-way crate
/// dependency and the VM never calls frontend, lowering, or optimizer APIs.
pub trait IncludeCompiler: std::fmt::Debug + Send + Sync {
    /// Identifies all compiler settings that affect generated code.
    fn fingerprint(&self, loader: &IncludeLoader) -> IncludeCompilerFingerprint;

    /// Compiles an include source already validated by the include loader.
    fn compile_include(
        &self,
        source: ValidatedIncludeSource,
        loader: &IncludeLoader,
    ) -> Result<CompiledInclude, VmError>;

    /// Compiles an in-memory eval source for immediate VM execution.
    fn compile_eval(&self, source_path: &str, source: &str) -> Result<CompiledUnit, VmError>;
}
