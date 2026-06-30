//! Stack frame and register storage for the first VM core.

use php_ir::IrSpan;
use php_ir::ids::{FunctionId, LocalId, RegId};
use php_runtime::{Lvalue, LvalueKind, ReferenceCell, Slot, TempValue, Value};

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
        self.locals.resize(count as usize, Slot::uninitialized());
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

    /// Writes a local without panicking.
    pub fn set(&mut self, id: LocalId, value: Value) -> Result<(), String> {
        let Some(slot) = self.locals.get_mut(id.index()) else {
            return Err(format!("invalid local local:{}", id.raw()));
        };
        Lvalue::slot(slot, LvalueKind::LocalVariable)
            .write_value(value)
            .map_err(|error| error.to_string())
    }

    /// Unsets a local name without writing through a referenced alias cell.
    pub fn unset(&mut self, id: LocalId) -> Result<(), String> {
        let Some(slot) = self.locals.get_mut(id.index()) else {
            return Err(format!("invalid local local:{}", id.raw()));
        };
        Lvalue::slot(slot, LvalueKind::LocalVariable)
            .unset()
            .map_err(|error| error.to_string())
    }

    /// Binds `target` to the same reference cell as `source`.
    pub fn bind_reference(&mut self, target: LocalId, source: LocalId) -> Result<(), String> {
        if target.index() >= self.locals.len() {
            return Err(format!("invalid local local:{}", target.raw()));
        }
        let Some(source_slot) = self.locals.get_mut(source.index()) else {
            return Err(format!("invalid local local:{}", source.raw()));
        };
        if source_slot.is_uninitialized() {
            source_slot.write(Value::Null);
        }
        let cell: ReferenceCell = Lvalue::slot(source_slot, LvalueKind::LocalVariable)
            .ensure_reference_cell()
            .map_err(|error| error.to_string())?;
        let target_slot = self
            .locals
            .get_mut(target.index())
            .expect("target bounds checked");
        Lvalue::slot(target_slot, LvalueKind::LocalVariable)
            .bind_reference_cell(cell)
            .map_err(|error| error.to_string())
    }

    /// Converts a local to a reference cell and returns that shared cell.
    pub fn ensure_reference_cell(&mut self, id: LocalId) -> Result<ReferenceCell, String> {
        let Some(slot) = self.locals.get_mut(id.index()) else {
            return Err(format!("invalid local local:{}", id.raw()));
        };
        if slot.is_uninitialized() {
            slot.write(Value::Null);
        }
        Lvalue::slot(slot, LvalueKind::LocalVariable)
            .ensure_reference_cell()
            .map_err(|error| error.to_string())
    }

    /// Binds a local to an existing reference cell.
    pub fn bind_reference_cell(&mut self, id: LocalId, cell: ReferenceCell) -> Result<(), String> {
        let Some(slot) = self.locals.get_mut(id.index()) else {
            return Err(format!("invalid local local:{}", id.raw()));
        };
        Lvalue::slot(slot, LvalueKind::LocalVariable)
            .bind_reference_cell(cell)
            .map_err(|error| error.to_string())
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
        self.registers
            .resize(count as usize, TempValue::uninitialized());
    }

    /// Reads a register without panicking.
    #[must_use]
    pub fn get(&self, id: RegId) -> Option<&Value> {
        self.registers.get(id.index()).map(TempValue::value)
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
    pub fn set(&mut self, id: RegId, value: Value) -> Result<(), String> {
        let Some(slot) = self.registers.get_mut(id.index()) else {
            return Err(format!("invalid register r{}", id.raw()));
        };
        slot.set(value);
        Ok(())
    }

    /// Clears a dead temporary register without affecting PHP-visible storage.
    pub fn unset(&mut self, id: RegId) -> Result<(), String> {
        let Some(slot) = self.registers.get_mut(id.index()) else {
            return Err(format!("invalid register r{}", id.raw()));
        };
        *slot = TempValue::uninitialized();
        Ok(())
    }
}

/// One executing frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    /// Function being executed.
    pub function: FunctionId,
    /// Class scope used for `self::`, visibility, and private member lookup.
    pub scope_class: Option<String>,
    /// Late-static-binding called class used for `static::`.
    pub called_class: Option<String>,
    /// Class that declares the selected method body.
    pub declaring_class: Option<String>,
    /// PHP-visible arguments supplied to this call after default/variadic
    /// normalization.
    pub arguments: Vec<Value>,
    /// Backtrace-visible arguments with redaction and named variadic labels.
    pub trace_arguments: Vec<FrameTraceArgument>,
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
    pub scope_class: Option<String>,
    /// Late-static-binding called class used for `static::`.
    pub called_class: Option<String>,
    /// Class that declares the selected method body.
    pub declaring_class: Option<String>,
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
            trace_arguments: Vec::new(),
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
            trace_arguments: Vec::new(),
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
        self.trace_arguments.clear();
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
        self.trace_arguments.clear();
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

    /// Returns the top frame.
    #[must_use]
    pub fn current(&self) -> Option<&Frame> {
        self.frames.last()
    }

    /// Returns the top frame mutably.
    pub fn current_mut(&mut self) -> Option<&mut Frame> {
        self.frames.last_mut()
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
