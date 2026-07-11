use super::{Vm, constant_value, layout_source};
use crate::bytecode::{DenseOperand, DenseOperandKind};
use crate::compiled_unit::CompiledUnit;
use crate::frame::CallStack;
use php_ir::ids::{ConstId, LocalId, RegId};
use php_ir::module::IrUnit;
use php_ir::operand::Operand;
use php_runtime::{Slot, Value, to_bool};

pub(super) enum DenseOperandRead<'a> {
    Borrowed(&'a Value),
    Owned(Value),
}

impl DenseOperandRead<'_> {
    pub(super) fn as_value(&self) -> &Value {
        match self {
            Self::Borrowed(value) => value,
            Self::Owned(value) => value,
        }
    }

    pub(super) fn into_owned(self) -> Value {
        match self {
            Self::Borrowed(value) => {
                let _source =
                    layout_source::enter_default(layout_source::STACK_REGISTER_LOCAL_MOVE);
                value.clone()
            }
            Self::Owned(value) => value,
        }
    }
}

impl Vm {
    pub(super) fn read_dense_operand(
        &self,
        compiled: &CompiledUnit,
        stack: &CallStack,
        operand: DenseOperand,
    ) -> Result<Value, String> {
        self.read_dense_operand_with_source(
            compiled,
            stack,
            operand,
            layout_source::STACK_REGISTER_LOCAL_MOVE,
        )
    }

    pub(super) fn take_consumed_dense_operand(
        &self,
        compiled: &CompiledUnit,
        stack: &mut CallStack,
        operand: DenseOperand,
    ) -> Result<Value, String> {
        if operand.kind != DenseOperandKind::Register {
            return self.read_dense_operand(compiled, stack, operand);
        }
        let frame = stack.current_mut().ok_or("no active frame")?;
        let value = frame.registers.take(RegId::new(operand.index))?;
        if value.is_uninitialized() {
            return Err(format!("read uninitialized register r{}", operand.index));
        }
        Ok(value)
    }

    pub(super) fn read_dense_operand_with_source(
        &self,
        compiled: &CompiledUnit,
        stack: &CallStack,
        operand: DenseOperand,
        source_family: php_runtime::layout_stats::LayoutSourceFamily,
    ) -> Result<Value, String> {
        match operand.kind {
            DenseOperandKind::Register => {
                let frame = stack.current().ok_or("no active frame")?;
                let Some(value) = frame.registers.get(RegId::new(operand.index)) else {
                    return Err(format!("invalid register r{}", operand.index));
                };
                if value.is_uninitialized() {
                    return Err(format!("read uninitialized register r{}", operand.index));
                }
                let _source = layout_source::enter(source_family);
                Ok(value.clone())
            }
            DenseOperandKind::Local => {
                let frame = stack.current().ok_or("no active frame")?;
                let _source = layout_source::enter(source_family);
                let Some(value) = frame.locals.get(LocalId::new(operand.index)) else {
                    return Err(format!("invalid local local:{}", operand.index));
                };
                Ok(if value.is_uninitialized() {
                    Value::Null
                } else {
                    value
                })
            }
            DenseOperandKind::Constant => {
                let id = ConstId::new(operand.index);
                if let Some(value) = self.resolved_constant_value(compiled, id) {
                    return Ok(value);
                }
                constant_value(compiled.unit(), id)
            }
        }
    }

    pub(super) fn read_dense_operand_ref<'a>(
        &self,
        compiled: &CompiledUnit,
        stack: &'a CallStack,
        operand: DenseOperand,
    ) -> Result<DenseOperandRead<'a>, String> {
        match operand.kind {
            DenseOperandKind::Register => {
                let frame = stack.current().ok_or("no active frame")?;
                let Some(value) = frame.registers.get(RegId::new(operand.index)) else {
                    return Err(format!("invalid register r{}", operand.index));
                };
                if value.is_uninitialized() {
                    return Err(format!("read uninitialized register r{}", operand.index));
                }
                Ok(DenseOperandRead::Borrowed(value))
            }
            DenseOperandKind::Local => {
                let frame = stack.current().ok_or("no active frame")?;
                let Some(slot) = frame.locals.get_slot(LocalId::new(operand.index)) else {
                    return Err(format!("invalid local local:{}", operand.index));
                };
                match slot {
                    Slot::Value(value) if value.is_uninitialized() => {
                        Ok(DenseOperandRead::Owned(Value::Null))
                    }
                    Slot::Value(value) => Ok(DenseOperandRead::Borrowed(value)),
                    Slot::Reference(cell) => {
                        self.record_counter_value_clone_reason("reference_or_cow");
                        let _source = layout_source::enter(layout_source::REFERENCE_DEREFERENCE);
                        Ok(DenseOperandRead::Owned(cell.get()))
                    }
                }
            }
            DenseOperandKind::Constant => {
                let id = ConstId::new(operand.index);
                if let Some(value) = self.resolved_constant_value(compiled, id) {
                    return Ok(DenseOperandRead::Owned(value));
                }
                constant_value(compiled.unit(), id).map(DenseOperandRead::Owned)
            }
        }
    }
}

pub(super) fn read_operand_at_frame(
    unit: &IrUnit,
    stack: &CallStack,
    frame_index: usize,
    operand: Operand,
) -> Result<Value, String> {
    match operand {
        Operand::Register(id) => {
            let frame = stack.frames().get(frame_index).ok_or("no active frame")?;
            let Some(value) = frame.registers.get(id) else {
                return Err(format!("invalid register r{}", id.raw()));
            };
            if value.is_uninitialized() {
                return Err(format!("read uninitialized register r{}", id.raw()));
            }
            let _source = layout_source::enter_default(layout_source::STACK_REGISTER_LOCAL_MOVE);
            Ok(value.clone())
        }
        Operand::Constant(id) => constant_value(unit, id),
        Operand::Local(id) => {
            let frame = stack.frames().get(frame_index).ok_or("no active frame")?;
            let _source = layout_source::enter_default(layout_source::STACK_REGISTER_LOCAL_MOVE);
            let Some(value) = frame.locals.get(id) else {
                return Err(format!("invalid local local:{}", id.raw()));
            };
            Ok(if value.is_uninitialized() {
                Value::Null
            } else {
                value
            })
        }
    }
}

pub(super) fn read_operand(
    unit: &IrUnit,
    stack: &CallStack,
    operand: Operand,
) -> Result<Value, String> {
    match operand {
        Operand::Register(id) => {
            let frame = stack.current().ok_or("no active frame")?;
            let Some(value) = frame.registers.get(id) else {
                return Err(format!("invalid register r{}", id.raw()));
            };
            if value.is_uninitialized() {
                return Err(format!("read uninitialized register r{}", id.raw()));
            }
            Ok(value.clone())
        }
        Operand::Constant(id) => constant_value(unit, id),
        Operand::Local(id) => {
            let frame = stack.current().ok_or("no active frame")?;
            let Some(value) = frame.locals.get(id) else {
                return Err(format!("invalid local local:{}", id.raw()));
            };
            Ok(if value.is_uninitialized() {
                Value::Null
            } else {
                value
            })
        }
    }
}

pub(super) fn unset_register_operand_at_frame(
    stack: &mut CallStack,
    frame_index: usize,
    operand: Operand,
) -> Result<(), String> {
    let Operand::Register(id) = operand else {
        return Ok(());
    };
    let frame = stack.frame_mut(frame_index).ok_or("no active frame")?;
    Ok(frame.registers.unset(id)?)
}

pub(super) fn unset_consumed_assignment_value_operand_at_frame(
    stack: &mut CallStack,
    frame_index: usize,
    value_operand: Operand,
    assignment_result: RegId,
) -> Result<(), String> {
    let Operand::Register(source) = value_operand else {
        return Ok(());
    };
    if source == assignment_result {
        return Ok(());
    }
    unset_register_operand_at_frame(stack, frame_index, value_operand)
}

pub(super) fn unset_dense_register_operand(
    stack: &mut CallStack,
    operand: DenseOperand,
) -> Result<(), String> {
    if operand.kind != DenseOperandKind::Register {
        return Ok(());
    }
    let frame = stack.current_mut().ok_or("no active frame")?;
    Ok(frame.registers.unset(RegId::new(operand.index))?)
}

pub(super) fn take_discard_operand_at_frame(
    unit: &IrUnit,
    stack: &mut CallStack,
    frame_index: usize,
    operand: Operand,
) -> Result<Option<Value>, String> {
    let Operand::Register(id) = operand else {
        return read_operand_at_frame(unit, stack, frame_index, operand).map(Some);
    };
    let frame = stack.frame_mut(frame_index).ok_or("no active frame")?;
    let value = frame.registers.take(id)?;
    if value.is_uninitialized() {
        return Ok(None);
    }
    Ok(Some(value))
}

pub(super) fn read_operand_ref_at_frame<'a>(
    unit: &IrUnit,
    stack: &'a CallStack,
    frame_index: usize,
    operand: Operand,
) -> Result<DenseOperandRead<'a>, String> {
    match operand {
        Operand::Register(id) => {
            let frame = stack.frames().get(frame_index).ok_or("no active frame")?;
            let Some(value) = frame.registers.get(id) else {
                return Err(format!("invalid register r{}", id.raw()));
            };
            if value.is_uninitialized() {
                return Err(format!("read uninitialized register r{}", id.raw()));
            }
            Ok(DenseOperandRead::Borrowed(value))
        }
        Operand::Constant(id) => constant_value(unit, id).map(DenseOperandRead::Owned),
        Operand::Local(id) => {
            let frame = stack.frames().get(frame_index).ok_or("no active frame")?;
            let Some(slot) = frame.locals.get_slot(id) else {
                return Err(format!("invalid local local:{}", id.raw()));
            };
            match slot {
                Slot::Value(value) if value.is_uninitialized() => {
                    Ok(DenseOperandRead::Owned(Value::Null))
                }
                Slot::Value(value) => Ok(DenseOperandRead::Borrowed(value)),
                Slot::Reference(cell) => Ok(DenseOperandRead::Owned(cell.get())),
            }
        }
    }
}

pub(super) fn operand_truthy_at_frame(
    unit: &IrUnit,
    stack: &CallStack,
    frame_index: usize,
    operand: Operand,
) -> Result<bool, String> {
    match operand {
        Operand::Register(id) => {
            let frame = stack.frames().get(frame_index).ok_or("no active frame")?;
            let Some(value) = frame.registers.get(id) else {
                return Err(format!("invalid register r{}", id.raw()));
            };
            if value.is_uninitialized() {
                return Err(format!("read uninitialized register r{}", id.raw()));
            }
            to_bool(value)
        }
        Operand::Constant(id) => {
            let value = constant_value(unit, id)?;
            to_bool(&value)
        }
        Operand::Local(id) => {
            let frame = stack.frames().get(frame_index).ok_or("no active frame")?;
            let Some(slot) = frame.locals.get_slot(id) else {
                return Err(format!("invalid local local:{}", id.raw()));
            };
            match slot {
                Slot::Value(value) if value.is_uninitialized() => to_bool(&Value::Null),
                Slot::Value(value) => to_bool(value),
                Slot::Reference(cell) => {
                    let value = cell.borrow();
                    to_bool(&value)
                }
            }
        }
    }
}
