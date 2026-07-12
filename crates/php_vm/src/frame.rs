//! Stack frame and register storage for the first VM core.

use std::sync::Arc;

use crate::error::VmError;
use php_ir::IrSpan;
use php_ir::ids::{FunctionId, LocalId, RegId};
use php_runtime::api::RuntimeSourceSpan;
use php_runtime::api::{Lvalue, LvalueError, LvalueKind, ReferenceCell, Slot, TempValue, Value};

/// Register storage with checked accessors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisterFile {
    registers: Vec<TempValue>,
}

/// Local storage with checked accessors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalFile {
    locals: Vec<Slot>,
}

impl LocalFile {
    /// Creates local storage filled with `Uninitialized`.
    #[must_use]
    pub fn new(count: u32) -> Self {
        Self {
            locals: vec![Slot::uninitialized(); count as usize],
        }
    }

    /// Reads a local without panicking.
    #[must_use]
    pub fn get(&self, id: LocalId) -> Option<Value> {
        self.locals.get(id.index()).map(Slot::read)
    }

    /// Returns true when the compiled local slot exists.
    #[must_use]
    pub fn contains(&self, id: LocalId) -> bool {
        id.index() < self.locals.len()
    }

    /// Iterates over local slots in stable slot order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (usize, &Slot)> {
        self.locals.iter().enumerate()
    }

    /// Clears all local values while keeping allocation capacity for reuse.
    pub fn clear_for_reuse(&mut self) {
        self.locals.clear();
    }

    /// Resizes local storage to the compiled slot count using existing capacity
    /// whenever possible.
    pub fn reset_for_function(&mut self, count: u32) {
        self.locals.clear();
        // `resize_with` constructs each slot in place; `resize` would clone a
        // template `Slot`, paying one counted `Value::clone` per local on
        // every frame reset (~a million per WordPress request).
        self.locals.resize_with(count as usize, Slot::uninitialized);
    }

    /// Reads a local slot mutably without panicking.
    pub fn get_slot_mut(&mut self, id: LocalId) -> Option<&mut Slot> {
        self.locals.get_mut(id.index())
    }

    /// Reads a local slot by reference without dereferencing or cloning it.
    #[must_use]
    pub fn get_slot(&self, id: LocalId) -> Option<&Slot> {
        self.locals.get(id.index())
    }

    /// Raw slot table for the audited unchecked accessors (ADR 0021).
    #[must_use]
    pub(crate) fn slot_table(&self) -> &[Slot] {
        &self.locals
    }

    /// Writes a local without panicking.
    pub fn set(&mut self, id: LocalId, value: Value) -> Result<(), VmError> {
        let Some(slot) = self.locals.get_mut(id.index()) else {
            return Err(invalid_local_error(id, "write"));
        };
        Lvalue::slot(slot, LvalueKind::LocalVariable)
            .write_value(value)
            .map_err(|error| lvalue_error(error, "write"))
    }

    /// Replaces a local slot without dereferencing reference storage.
    pub fn set_slot(&mut self, id: LocalId, value: Slot) -> Result<(), VmError> {
        let Some(slot) = self.locals.get_mut(id.index()) else {
            return Err(invalid_local_error(id, "write_slot"));
        };
        *slot = value;
        Ok(())
    }

    /// Writes a local and attaches source context to any VM access error.
    pub fn set_with_span(
        &mut self,
        id: LocalId,
        value: Value,
        span: RuntimeSourceSpan,
    ) -> Result<(), VmError> {
        self.set(id, value).map_err(|error| error.with_span(span))
    }

    /// Unsets a local name without writing through a referenced alias cell.
    pub fn unset(&mut self, id: LocalId) -> Result<(), VmError> {
        let Some(slot) = self.locals.get_mut(id.index()) else {
            return Err(invalid_local_error(id, "unset"));
        };
        Lvalue::slot(slot, LvalueKind::LocalVariable)
            .unset()
            .map_err(|error| lvalue_error(error, "unset"))
    }

    /// Unsets a local and attaches source context to any VM access error.
    pub fn unset_with_span(&mut self, id: LocalId, span: RuntimeSourceSpan) -> Result<(), VmError> {
        self.unset(id).map_err(|error| error.with_span(span))
    }

    /// Binds `target` to the same reference cell as `source`.
    pub fn bind_reference(&mut self, target: LocalId, source: LocalId) -> Result<(), VmError> {
        if target.index() >= self.locals.len() {
            return Err(invalid_local_error(target, "bind_reference_target"));
        }
        let Some(source_slot) = self.locals.get_mut(source.index()) else {
            return Err(invalid_local_error(source, "bind_reference_source"));
        };
        if source_slot.is_uninitialized() {
            source_slot.write(Value::Null);
        }
        let cell: ReferenceCell = Lvalue::slot(source_slot, LvalueKind::LocalVariable)
            .ensure_reference_cell()
            .map_err(|error| lvalue_error(error, "bind_reference_source"))?;
        let target_slot = self
            .locals
            .get_mut(target.index())
            .expect("target bounds checked");
        Lvalue::slot(target_slot, LvalueKind::LocalVariable)
            .bind_reference_cell(cell)
            .map_err(|error| lvalue_error(error, "bind_reference_target"))
    }

    /// Converts a local to a reference cell and returns that shared cell.
    pub fn ensure_reference_cell(&mut self, id: LocalId) -> Result<ReferenceCell, VmError> {
        let Some(slot) = self.locals.get_mut(id.index()) else {
            return Err(invalid_local_error(id, "ensure_reference_cell"));
        };
        if slot.is_uninitialized() {
            slot.write(Value::Null);
        }
        Lvalue::slot(slot, LvalueKind::LocalVariable)
            .ensure_reference_cell()
            .map_err(|error| lvalue_error(error, "ensure_reference_cell"))
    }

    /// Binds a local to an existing reference cell.
    pub fn bind_reference_cell(&mut self, id: LocalId, cell: ReferenceCell) -> Result<(), VmError> {
        let Some(slot) = self.locals.get_mut(id.index()) else {
            return Err(invalid_local_error(id, "bind_reference_cell"));
        };
        Lvalue::slot(slot, LvalueKind::LocalVariable)
            .bind_reference_cell(cell)
            .map_err(|error| lvalue_error(error, "bind_reference_cell"))
    }
}

impl RegisterFile {
    /// Creates a register file filled with `Uninitialized`.
    #[must_use]
    pub fn new(count: u32) -> Self {
        Self {
            registers: vec![TempValue::uninitialized(); count as usize],
        }
    }

    /// Returns the number of registers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.registers.len()
    }

    /// Returns true when no registers are allocated.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.registers.is_empty()
    }

    /// Clears all register values while keeping allocation capacity for reuse.
    pub fn clear_for_reuse(&mut self) {
        self.registers.clear();
    }

    /// Resizes register storage to the compiled register count using existing
    /// capacity whenever possible.
    pub fn reset_for_function(&mut self, count: u32) {
        self.registers.clear();
        // See `LocalFile::reset_for_function`: construct in place instead of
        // cloning a template per register.
        self.registers
            .resize_with(count as usize, TempValue::uninitialized);
    }

    /// Reads a register without panicking.
    #[must_use]
    pub fn get(&self, id: RegId) -> Option<&Value> {
        self.registers.get(id.index()).map(TempValue::value)
    }

    /// Raw slot table for the audited unchecked accessors (ADR 0021).
    #[must_use]
    pub(crate) fn temp_slots(&self) -> &[TempValue] {
        &self.registers
    }

    /// Mutable raw slot table for the audited unchecked accessors (ADR 0021).
    pub(crate) fn temp_slots_mut(&mut self) -> &mut [TempValue] {
        &mut self.registers
    }

    /// Iterates over registers in stable register order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (usize, &Value)> {
        self.registers
            .iter()
            .enumerate()
            .map(|(index, value)| (index, value.value()))
    }

    /// Reads a register mutably without panicking.
    pub fn get_mut(&mut self, id: RegId) -> Option<&mut Value> {
        self.registers.get_mut(id.index()).map(TempValue::value_mut)
    }

    /// Writes a register without panicking.
    pub fn set(&mut self, id: RegId, value: Value) -> Result<(), VmError> {
        let Some(slot) = self.registers.get_mut(id.index()) else {
            return Err(invalid_register_error(id, "write"));
        };
        slot.set(value);
        Ok(())
    }

    /// Consumes a dead temporary register and marks it uninitialized.
    pub fn take(&mut self, id: RegId) -> Result<Value, VmError> {
        let Some(slot) = self.registers.get_mut(id.index()) else {
            return Err(invalid_register_error(id, "take"));
        };
        Ok(std::mem::replace(slot, TempValue::uninitialized()).into_value())
    }

    /// Writes a register and attaches source context to any VM access error.
    pub fn set_with_span(
        &mut self,
        id: RegId,
        value: Value,
        span: RuntimeSourceSpan,
    ) -> Result<(), VmError> {
        self.set(id, value).map_err(|error| error.with_span(span))
    }

    /// Clears a dead temporary register without affecting PHP-visible storage.
    pub fn unset(&mut self, id: RegId) -> Result<(), VmError> {
        let Some(slot) = self.registers.get_mut(id.index()) else {
            return Err(invalid_register_error(id, "unset"));
        };
        *slot = TempValue::uninitialized();
        Ok(())
    }

    /// Clears a register and attaches source context to any VM access error.
    pub fn unset_with_span(&mut self, id: RegId, span: RuntimeSourceSpan) -> Result<(), VmError> {
        self.unset(id).map_err(|error| error.with_span(span))
    }
}

fn invalid_local_error(id: LocalId, operation: &'static str) -> VmError {
    VmError::internal(
        "E_PHP_VM_INVALID_LOCAL_SLOT",
        "frame",
        format!("invalid local slot l{}", id.raw()),
    )
    .with_context("slot", id.raw())
    .with_context("operation", operation)
}

fn lvalue_error(error: LvalueError, operation: &'static str) -> VmError {
    match error {
        LvalueError::CannotRebindCell { kind } => VmError::internal(
            "E_PHP_VM_LVALUE_REBIND_CELL",
            "frame",
            format!("cannot rebind {} cell storage", kind.as_str()),
        )
        .with_context("operation", operation)
        .with_context("kind", kind.as_str()),
    }
}

fn invalid_register_error(id: RegId, operation: &'static str) -> VmError {
    VmError::internal(
        "E_PHP_VM_INVALID_REGISTER_SLOT",
        "frame",
        format!("invalid register slot r{}", id.raw()),
    )
    .with_context("slot", id.raw())
    .with_context("operation", operation)
}

/// One executing frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    /// Function being executed.
    pub function: FunctionId,
    /// Class scope used for `self::`, visibility, and private member lookup.
    pub scope_class: Option<Arc<str>>,
    /// Late-static-binding called class used for `static::`.
    pub called_class: Option<Arc<str>>,
    /// Class that declares the selected method body.
    pub declaring_class: Option<Arc<str>>,
    /// PHP-visible arguments supplied to this call after default/variadic
    /// normalization.
    pub arguments: Vec<Value>,
    /// Backtrace-visible arguments stored in the internal `TraceArguments` form.
    pub trace_arguments: TraceArguments,
    /// Source span of the call site that activated this frame, when known.
    pub call_span: Option<IrSpan>,
    /// Registers for the function.
    pub registers: RegisterFile,
    /// PHP local variable slots for the function.
    pub locals: LocalFile,
    /// True when this activation may be returned to the request-local frame
    /// pool after it completes.
    pub reuse_eligible: bool,
}

/// Metadata attached to a function activation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FrameActivationContext {
    /// Class scope used for `self::`, visibility, and private member lookup.
    pub scope_class: Option<Arc<str>>,
    /// Late-static-binding called class used for `static::`.
    pub called_class: Option<Arc<str>>,
    /// Class that declares the selected method body.
    pub declaring_class: Option<Arc<str>>,
    /// Source span of the call site that activated this frame, when known.
    pub call_span: Option<IrSpan>,
}

impl Frame {
    /// Creates a frame for a function.
    #[must_use]
    pub fn new(function: FunctionId, register_count: u32, local_count: u32) -> Self {
        Self {
            function,
            scope_class: None,
            called_class: None,
            declaring_class: None,
            arguments: Vec::new(),
            trace_arguments: TraceArguments::default(),
            call_span: None,
            registers: RegisterFile::new(register_count),
            locals: LocalFile::new(local_count),
            reuse_eligible: false,
        }
    }

    /// Creates a frame for a class method with explicit class metadata.
    #[must_use]
    pub fn new_with_activation_context(
        function: FunctionId,
        register_count: u32,
        local_count: u32,
        context: FrameActivationContext,
    ) -> Self {
        Self {
            function,
            scope_class: context.scope_class,
            called_class: context.called_class,
            declaring_class: context.declaring_class,
            arguments: Vec::new(),
            trace_arguments: TraceArguments::default(),
            call_span: context.call_span,
            registers: RegisterFile::new(register_count),
            locals: LocalFile::new(local_count),
            reuse_eligible: false,
        }
    }

    /// Clears PHP-visible values before moving a frame into the request-local
    /// reuse pool. Capacities are retained but roots are dropped.
    pub fn clear_for_reuse(&mut self) {
        self.scope_class = None;
        self.called_class = None;
        self.declaring_class = None;
        self.call_span = None;
        self.arguments.clear();
        self.trace_arguments = TraceArguments::default();
        self.registers.clear_for_reuse();
        self.locals.clear_for_reuse();
        self.reuse_eligible = false;
    }

    /// Reinitializes a pooled frame for a new function activation.
    pub fn reset_with_activation_context(
        &mut self,
        function: FunctionId,
        register_count: u32,
        local_count: u32,
        context: FrameActivationContext,
    ) {
        self.function = function;
        self.scope_class = context.scope_class;
        self.called_class = context.called_class;
        self.declaring_class = context.declaring_class;
        self.call_span = context.call_span;
        self.arguments.clear();
        self.trace_arguments = TraceArguments::default();
        self.registers.reset_for_function(register_count);
        self.locals.reset_for_function(local_count);
        self.reuse_eligible = false;
    }
}

/// One argument as PHP stack traces should render it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameTraceArgument {
    pub name: Option<String>,
    pub value: Value,
}

/// Backtrace-visible arguments of a frame.
///
/// The reference engine reports the *current* argument slot values (parameter
/// mutations are visible in `debug_backtrace()` and exception traces), so the
/// normal case reconstructs them lazily from the live locals when a trace is
/// actually requested — no per-call snapshot clones. Frames whose locals never
/// bind the call arguments (native leaf frames) carry an eager snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TraceArguments {
    /// Reconstruct on demand from live locals plus the `arguments` overflow;
    /// `arg_count` is the PHP-visible call arity (`func_num_args`), so
    /// defaulted parameters never appear.
    Lazy { arg_count: u32 },
    /// Eager snapshot for frames without argument-bound locals.
    Materialized(Vec<FrameTraceArgument>),
}

impl Default for TraceArguments {
    fn default() -> Self {
        Self::Lazy { arg_count: 0 }
    }
}

/// Minimal call stack container.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CallStack {
    frames: Vec<Frame>,
    frame_pool: Vec<Frame>,
}

impl CallStack {
    /// Creates an empty call stack.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            frames: Vec::new(),
            frame_pool: Vec::new(),
        }
    }

    /// Pushes a frame.
    pub fn push(&mut self, frame: Frame) {
        self.frames.push(frame);
    }

    /// Pushes a compiled function frame that must not enter the reuse pool.
    pub fn push_fresh_frame(
        &mut self,
        function: FunctionId,
        register_count: u32,
        local_count: u32,
        context: FrameActivationContext,
    ) {
        let mut frame =
            Frame::new_with_activation_context(function, register_count, local_count, context);
        frame.reuse_eligible = false;
        self.frames.push(frame);
    }

    /// Pushes a compiled function frame, reusing request-local storage when a
    /// completed frame is available. Returns true when reuse happened.
    pub fn push_reusable_frame(
        &mut self,
        function: FunctionId,
        register_count: u32,
        local_count: u32,
        context: FrameActivationContext,
    ) -> bool {
        if let Some(mut frame) = self.frame_pool.pop() {
            frame.reset_with_activation_context(function, register_count, local_count, context);
            frame.reuse_eligible = true;
            self.frames.push(frame);
            return true;
        }

        let mut frame =
            Frame::new_with_activation_context(function, register_count, local_count, context);
        frame.reuse_eligible = true;
        self.frames.push(frame);
        false
    }

    /// Pops a frame.
    pub fn pop(&mut self) -> Option<Frame> {
        self.frames.pop()
    }

    /// Pops a frame by stack depth from entry to current frame.
    pub fn pop_frame(&mut self, index: usize) -> Option<Frame> {
        if index >= self.frames.len() {
            return None;
        }
        Some(self.frames.remove(index))
    }

    /// Pops a completed frame into the request-local reuse pool.
    pub fn pop_recycle(&mut self) -> bool {
        let Some(mut frame) = self.frames.pop() else {
            return false;
        };
        if !frame.reuse_eligible {
            return false;
        }
        frame.clear_for_reuse();
        self.frame_pool.push(frame);
        true
    }

    /// Pops a completed frame by stack depth into the request-local reuse pool.
    pub fn pop_frame_recycle(&mut self, index: usize) -> bool {
        let Some(mut frame) = self.pop_frame(index) else {
            return false;
        };
        if !frame.reuse_eligible {
            return false;
        }
        frame.clear_for_reuse();
        self.frame_pool.push(frame);
        true
    }

    /// Returns the top frame.
    #[must_use]
    pub fn current(&self) -> Option<&Frame> {
        self.frames.last()
    }

    /// Returns the top frame mutably.
    pub fn current_mut(&mut self) -> Option<&mut Frame> {
        self.frames.last_mut()
    }

    /// Returns a mutable frame by stack depth from entry to current frame.
    pub fn frame_mut(&mut self, index: usize) -> Option<&mut Frame> {
        self.frames.get_mut(index)
    }

    /// Returns frames from entry to current frame.
    #[must_use]
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// Returns the number of frames.
    #[must_use]
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// Returns true when no frames are active.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{CallStack, Frame, FrameActivationContext, FrameTraceArgument, TraceArguments};
    use php_ir::IrSpan;
    use php_ir::ids::{FileId, FunctionId, LocalId, RegId};
    use php_runtime::api::RuntimeSourceSpan;
    use php_runtime::api::{
        ClassEntry, ClassFlags, ObjectRef, PhpArray, ReferenceCell, Slot, Value,
    };

    fn empty_class(name: &str) -> ClassEntry {
        ClassEntry {
            name: name.to_owned().into(),
            parent: None,
            interfaces: Vec::new(),
            methods: Vec::new(),
            properties: Vec::new(),
            constants: Vec::new(),
            enum_cases: Vec::new(),
            attributes: Vec::new(),
            enum_backing_type: None,
            constructor_id: None,
            flags: ClassFlags::default(),
        }
    }

    fn activation(scope: &str, start: u32) -> FrameActivationContext {
        FrameActivationContext {
            scope_class: Some(Arc::from(scope)),
            called_class: Some(Arc::from(format!("{scope}Called"))),
            declaring_class: Some(Arc::from(format!("{scope}Decl"))),
            call_span: Some(IrSpan::new(FileId::new(1), start, start + 1)),
        }
    }

    #[test]
    fn recycled_frames_drop_php_visible_roots_before_pooling() {
        let mut stack = CallStack::new();
        let class = empty_class("FrameRoot");
        let (reference_handle, array_handle, object_handle) = {
            let object = ObjectRef::new(&class);
            let object_handle = object.weak_handle();
            let array = PhpArray::from_packed(vec![Value::Object(object)]);
            let array_handle = array.weak_handle();
            let cell = ReferenceCell::new(Value::Array(array));
            let reference_handle = cell.weak_handle();
            let mut frame =
                Frame::new_with_activation_context(FunctionId::new(1), 2, 2, activation("Old", 3));
            frame.reuse_eligible = true;
            frame.arguments.push(Value::Reference(cell.clone()));
            frame.trace_arguments = TraceArguments::Materialized(vec![FrameTraceArgument {
                name: Some("arg".to_owned()),
                value: Value::Reference(cell.clone()),
            }]);
            frame
                .registers
                .set(RegId::new(0), Value::Reference(cell.clone()))
                .expect("register write");
            frame
                .locals
                .bind_reference_cell(LocalId::new(0), cell)
                .expect("local reference bind");
            stack.push(frame);
            (reference_handle, array_handle, object_handle)
        };

        assert!(reference_handle.is_alive());
        assert!(array_handle.is_alive());
        assert!(object_handle.is_alive());

        assert!(stack.pop_recycle());

        assert!(!reference_handle.is_alive());
        assert!(!array_handle.is_alive());
        assert!(!object_handle.is_alive());
    }

    #[test]
    fn reusable_frames_reset_metadata_arguments_locals_and_registers() {
        let mut stack = CallStack::new();
        let mut frame =
            Frame::new_with_activation_context(FunctionId::new(1), 2, 2, activation("Old", 10));
        frame.reuse_eligible = true;
        frame.arguments.push(Value::Int(1));
        frame.trace_arguments = TraceArguments::Materialized(vec![FrameTraceArgument {
            name: None,
            value: Value::Int(2),
        }]);
        frame
            .registers
            .set(RegId::new(0), Value::string("old-register"))
            .expect("register write");
        frame
            .locals
            .set(LocalId::new(0), Value::string("old-local"))
            .expect("local write");
        stack.push(frame);

        assert!(stack.pop_recycle());
        assert!(stack.push_reusable_frame(FunctionId::new(2), 1, 1, activation("New", 20)));

        let frame = stack.current().expect("reused frame");
        assert_eq!(frame.function, FunctionId::new(2));
        assert_eq!(frame.scope_class.as_deref(), Some("New"));
        assert_eq!(frame.called_class.as_deref(), Some("NewCalled"));
        assert_eq!(frame.declaring_class.as_deref(), Some("NewDecl"));
        assert_eq!(frame.call_span, Some(IrSpan::new(FileId::new(1), 20, 21)));
        assert!(frame.arguments.is_empty());
        assert_eq!(frame.trace_arguments, TraceArguments::default());
        assert_eq!(frame.registers.len(), 1);
        assert_eq!(
            frame.registers.get(RegId::new(0)),
            Some(&Value::Uninitialized)
        );
        assert_eq!(frame.locals.iter().len(), 1);
        assert_eq!(
            frame.locals.get(LocalId::new(0)),
            Some(Value::Uninitialized)
        );
    }

    #[test]
    fn non_reusable_frames_never_enter_the_pool() {
        let mut stack = CallStack::new();
        stack.push_fresh_frame(FunctionId::new(1), 1, 1, activation("Fresh", 30));

        assert!(!stack.pop_recycle());
        assert!(!stack.push_reusable_frame(FunctionId::new(2), 1, 1, activation("Next", 40)));
        assert_eq!(
            stack.current().expect("new frame").function,
            FunctionId::new(2)
        );
    }

    #[test]
    fn register_and_local_invalid_access_returns_typed_vm_errors() {
        let mut frame = Frame::new(FunctionId::new(1), 0, 0);

        let register_error = frame
            .registers
            .set(RegId::new(7), Value::Int(1))
            .expect_err("invalid register");
        assert_eq!(register_error.code(), "E_PHP_VM_INVALID_REGISTER_SLOT");
        assert_eq!(
            register_error
                .context()
                .get("operation")
                .map(String::as_str),
            Some("write")
        );

        let local_error = frame
            .locals
            .set(LocalId::new(9), Value::Int(1))
            .expect_err("invalid local");
        assert_eq!(local_error.code(), "E_PHP_VM_INVALID_LOCAL_SLOT");
        assert_eq!(
            local_error.context().get("operation").map(String::as_str),
            Some("write")
        );

        assert_eq!(frame.locals.get_slot(LocalId::new(0)), None);
        assert_eq!(frame.locals.get_slot_mut(LocalId::new(0)), None);
        assert_eq!(
            frame
                .locals
                .iter()
                .map(|(_, slot)| slot)
                .collect::<Vec<&Slot>>(),
            Vec::<&Slot>::new()
        );
    }

    #[test]
    fn register_and_local_span_helpers_attach_source_context() {
        let mut frame = Frame::new(FunctionId::new(1), 0, 0);
        let span = RuntimeSourceSpan {
            file: Some("fixture.php".to_owned()),
            start: 12,
            end: 18,
        };

        let register_error = frame
            .registers
            .unset_with_span(RegId::new(2), span.clone())
            .expect_err("invalid register");
        assert_eq!(register_error.code(), "E_PHP_VM_INVALID_REGISTER_SLOT");
        assert_eq!(register_error.source_span(), Some(&span));

        let local_error = frame
            .locals
            .set_with_span(LocalId::new(3), Value::Int(1), span.clone())
            .expect_err("invalid local");
        assert_eq!(local_error.code(), "E_PHP_VM_INVALID_LOCAL_SLOT");
        assert_eq!(local_error.source_span(), Some(&span));
    }
}
