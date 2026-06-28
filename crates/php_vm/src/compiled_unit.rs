//! VM-facing wrapper around verified IR units.

use php_ir::constants::IrConstant;
use php_ir::ids::FunctionId;
use php_ir::module::{ClassEntry, normalize_class_name};
use php_ir::{ConstId, IrUnit};

/// VM-facing function lookup entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledFunctionEntry {
    /// Normalized lookup name.
    pub name: String,
    /// Function ID.
    pub function: FunctionId,
}

/// VM-facing constant lookup entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledConstantEntry {
    /// Canonical runtime lookup name.
    pub name: String,
    /// Constant-pool value ID.
    pub value: ConstId,
}

/// Compiled unit handed to the interpreter.
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledUnit {
    unit: IrUnit,
    function_table: Vec<CompiledFunctionEntry>,
    constant_table: Vec<CompiledConstantEntry>,
    class_table: Vec<ClassEntry>,
}

impl CompiledUnit {
    /// Wraps an IR unit for execution.
    #[must_use]
    pub fn new(unit: IrUnit) -> Self {
        let function_table = unit
            .function_table
            .iter()
            .map(|entry| CompiledFunctionEntry {
                name: entry.name.clone(),
                function: entry.function,
            })
            .collect();
        let constant_table = unit
            .constant_table
            .iter()
            .map(|entry| CompiledConstantEntry {
                name: entry.name.clone(),
                value: entry.value,
            })
            .collect();
        let class_table = unit.classes.clone();
        Self {
            unit,
            function_table,
            constant_table,
            class_table,
        }
    }

    /// Returns the underlying IR unit.
    #[must_use]
    pub const fn unit(&self) -> &IrUnit {
        &self.unit
    }

    /// Finds a user function by normalized name.
    #[must_use]
    pub fn lookup_function(&self, name: &str) -> Option<FunctionId> {
        self.function_table
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.function)
    }

    /// Finds a user constant by canonical name.
    #[must_use]
    pub fn lookup_constant(&self, name: &str) -> Option<&IrConstant> {
        let value = self
            .constant_table
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| entry.value)?;
        self.unit.constants.get(value.index())
    }

    /// Finds a class by normalized name.
    #[must_use]
    pub fn lookup_class(&self, name: &str) -> Option<&ClassEntry> {
        let normalized = normalize_class_name(name);
        self.class_table
            .iter()
            .find(|entry| normalize_class_name(&entry.name) == normalized)
    }

    /// Returns the VM lookup table.
    #[must_use]
    pub fn function_table(&self) -> &[CompiledFunctionEntry] {
        &self.function_table
    }

    /// Returns the VM constant lookup table.
    #[must_use]
    pub fn constant_table(&self) -> &[CompiledConstantEntry] {
        &self.constant_table
    }

    /// Returns the VM class lookup table.
    #[must_use]
    pub fn class_table(&self) -> &[ClassEntry] {
        &self.class_table
    }

    /// Consumes the wrapper and returns the IR unit.
    #[must_use]
    pub fn into_unit(self) -> IrUnit {
        self.unit
    }
}

impl From<IrUnit> for CompiledUnit {
    fn from(unit: IrUnit) -> Self {
        Self::new(unit)
    }
}
