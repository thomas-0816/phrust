use super::prelude::*;

/// Borrows the mutable state associated with one active VM execution path.
pub(super) struct ExecutionCursor<'a> {
    pub(super) compiled: &'a CompiledUnit,
    pub(super) output: &'a mut OutputBuffer,
    pub(super) stack: &'a mut CallStack,
    pub(super) state: &'a mut ExecutionState,
}

impl<'a> ExecutionCursor<'a> {
    pub(super) fn new(
        compiled: &'a CompiledUnit,
        output: &'a mut OutputBuffer,
        stack: &'a mut CallStack,
        state: &'a mut ExecutionState,
    ) -> Self {
        Self {
            compiled,
            output,
            stack,
            state,
        }
    }
}

/// Read-only execution data used while formatting diagnostics and traces.
pub(super) struct ExecutionView<'a> {
    pub(super) compiled: &'a CompiledUnit,
    pub(super) output: &'a OutputBuffer,
    pub(super) stack: &'a CallStack,
    pub(super) state: &'a ExecutionState,
}

impl<'a> ExecutionView<'a> {
    pub(super) fn new(
        compiled: &'a CompiledUnit,
        output: &'a OutputBuffer,
        stack: &'a CallStack,
        state: &'a ExecutionState,
    ) -> Self {
        Self {
            compiled,
            output,
            stack,
            state,
        }
    }
}
