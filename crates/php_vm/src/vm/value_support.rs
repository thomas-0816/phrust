use super::*;

pub(super) fn output_preallocation_hint(unit: &IrUnit) -> usize {
    unit.functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter_map(|instruction| match instruction.kind {
            InstructionKind::Echo {
                src: Operand::Constant(id),
            } => unit.constants.get(id.index()),
            _ => None,
        })
        .filter_map(|constant| match constant {
            IrConstant::String(value) => Some(value.len()),
            IrConstant::StringBytes(value) => Some(value.len()),
            _ => None,
        })
        .sum()
}

pub(super) fn ir_unit_instruction_count(unit: &IrUnit) -> u32 {
    unit.functions
        .iter()
        .flat_map(|function| function.blocks.iter())
        .map(|block| block.instructions.len() as u32 + u32::from(block.terminator.is_some()))
        .sum()
}

pub(super) fn constant_value(unit: &IrUnit, constant: ConstId) -> Result<Value, String> {
    let Some(value) = unit.constants.get(constant.index()) else {
        return Err(format!("invalid constant const:{}", constant.raw()));
    };
    Ok(match value {
        IrConstant::Null => Value::Null,
        IrConstant::Bool(value) => Value::Bool(*value),
        IrConstant::Int(value) => Value::Int(*value),
        IrConstant::Float(value) => Value::float(*value),
        IrConstant::String(value) => Value::String(PhpString::from_test_str(value)),
        IrConstant::StringBytes(value) => Value::String(PhpString::from_bytes(value.clone())),
        IrConstant::NamedConstant(name) => {
            return Err(format!(
                "E_PHP_VM_UNRESOLVED_CONSTANT_EXPR: constant {name} requires runtime resolution"
            ));
        }
        IrConstant::ClassConstant {
            class_name,
            constant_name,
        } => {
            return Err(format!(
                "E_PHP_VM_UNRESOLVED_CONSTANT_EXPR: constant {class_name}::{constant_name} requires runtime resolution"
            ));
        }
        IrConstant::Array(entries) => {
            let mut array = PhpArray::new();
            for entry in entries {
                let value = inline_constant_value(&entry.value);
                if let Some(key) = &entry.key {
                    let key_value = inline_constant_value(key);
                    if let Some(key) = ArrayKey::from_value(&key_value) {
                        array.insert(key, value);
                    } else {
                        array.append(value);
                    }
                } else {
                    array.append(value);
                }
            }
            Value::Array(array)
        }
    })
}

pub(super) fn inline_constant_value(constant: &IrConstant) -> Value {
    match constant {
        IrConstant::Null => Value::Null,
        IrConstant::Bool(value) => Value::Bool(*value),
        IrConstant::Int(value) => Value::Int(*value),
        IrConstant::Float(value) => Value::float(*value),
        IrConstant::String(value) => Value::String(PhpString::from_test_str(value)),
        IrConstant::StringBytes(value) => Value::String(PhpString::from_bytes(value.clone())),
        IrConstant::NamedConstant(_) | IrConstant::ClassConstant { .. } => Value::Null,
        IrConstant::Array(entries) => {
            let mut array = PhpArray::new();
            for entry in entries {
                let value = inline_constant_value(&entry.value);
                if let Some(key) = &entry.key {
                    let key_value = inline_constant_value(key);
                    if let Some(key) = ArrayKey::from_value(&key_value) {
                        array.insert(key, value);
                    } else {
                        array.append(value);
                    }
                } else {
                    array.append(value);
                }
            }
            Value::Array(array)
        }
    }
}

pub(super) fn array_key_from_value(value: &Value) -> Result<ArrayKey, String> {
    ArrayKey::from_value(value).ok_or_else(|| {
        if let Value::Object(object) = effective_value(value) {
            return format!(
                "E_PHP_VM_ARRAY_KEY_CONVERSION: Cannot access offset of type {} on array",
                object.display_name()
            );
        }
        format!(
            "E_PHP_VM_ARRAY_KEY_CONVERSION: cannot use {} as array key",
            value_type_name(value)
        )
    })
}

pub(super) fn array_key_to_value(key: ArrayKey) -> Value {
    match key {
        ArrayKey::Int(value) => Value::Int(value),
        ArrayKey::String(value) => Value::String(value),
    }
}

pub(super) fn clone_with_property_name(key: &ArrayKey) -> Result<String, String> {
    let ArrayKey::String(value) = key else {
        return Err(
            "E_PHP_VM_CLONE_WITH_PROPERTY_KEY: clone-with property names must be strings"
                .to_owned(),
        );
    };
    String::from_utf8(value.as_bytes().to_vec()).map_err(|_| {
        "E_PHP_VM_CLONE_WITH_PROPERTY_KEY: clone-with property name is not valid UTF-8".to_owned()
    })
}

/// Parse the leading PHP integer of a non-canonical string array key, used to
/// resolve string offsets like `$s["0foo"]`. Returns `None` for keys with no
/// leading integer (e.g. `"foo"`), which the caller treats as a non-numeric
/// offset.
pub(super) fn leading_int_offset(bytes: &[u8]) -> Option<i64> {
    let mut index = 0;
    let mut negative = false;
    if matches!(bytes.first(), Some(b'-' | b'+')) {
        negative = bytes[0] == b'-';
        index = 1;
    }
    let digits_start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == digits_start {
        return None;
    }
    let digits = std::str::from_utf8(&bytes[digits_start..index]).ok()?;
    let magnitude = digits.parse::<i64>().ok()?;
    Some(if negative { -magnitude } else { magnitude })
}

/// Read a single-byte string offset following PHP semantics: integer keys may be
/// negative (counted from the end), and string keys use their leading integer.
/// Returns `None` for out-of-range or non-numeric offsets.
pub(super) fn string_offset_byte(string: &PhpString, key: &ArrayKey) -> Option<Value> {
    let index = match key {
        ArrayKey::Int(value) => *value,
        ArrayKey::String(value) => leading_int_offset(value.as_bytes())?,
    };
    let length = string.len() as i64;
    let resolved = if index < 0 { index + length } else { index };
    if resolved < 0 || resolved >= length {
        return None;
    }
    Some(Value::string(vec![string.as_bytes()[resolved as usize]]))
}

/// Outcome of reading a string offset, distinguishing the diagnostics PHP emits.
pub(super) enum StringOffsetRead {
    /// In-range read with an integer (or canonical integer string) key.
    Byte(Value),
    /// Integer offset outside the string; PHP warns "Uninitialized string offset".
    OutOfRange(i64),
    /// Leading-integer string key (e.g. `"0foo"`); PHP warns "Illegal string offset".
    Illegal { value: Value, key_bytes: Vec<u8> },
    /// Non-numeric string key; PHP throws TypeError on read, false on isset.
    NonNumeric,
}

pub(super) fn string_offset_for_read(string: &PhpString, key: &ArrayKey) -> StringOffsetRead {
    let (index, illegal_key) = match key {
        ArrayKey::Int(value) => (*value, None),
        ArrayKey::String(value) => match leading_int_offset(value.as_bytes()) {
            Some(index) => (index, Some(value.as_bytes().to_vec())),
            None => return StringOffsetRead::NonNumeric,
        },
    };
    let length = string.len() as i64;
    let resolved = if index < 0 { index + length } else { index };
    let byte = if resolved < 0 || resolved >= length {
        None
    } else {
        Some(Value::string(vec![string.as_bytes()[resolved as usize]]))
    };
    match (illegal_key, byte) {
        (Some(key_bytes), value) => StringOffsetRead::Illegal {
            value: value.unwrap_or_else(|| Value::string(Vec::new())),
            key_bytes,
        },
        (None, Some(value)) => StringOffsetRead::Byte(value),
        (None, None) => StringOffsetRead::OutOfRange(index),
    }
}

/// Rich-IR instruction kinds with a quickening candidate or guard arm in the
/// dispatch loop (int add/sub/mul, string concat, packed-array int-key fetch).
/// Observing any other kind cannot lead to a specialization: it only grows the
/// per-site ordered map with write-only entries on the dispatch hot path and
/// reports phantom `specialized` events once a site crosses the execution
/// threshold, so per-instruction observation is limited to these kinds.
pub(super) fn rich_quickening_candidate_kind(kind: &InstructionKind) -> bool {
    matches!(
        kind,
        InstructionKind::Binary {
            op: BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Concat,
            ..
        } | InstructionKind::FetchDim { .. }
    )
}

/// Dense opcodes with a quickening candidate or guard arm in the dispatch
/// loop. Keeping this exhaustive allowlist next to the rich classifier avoids
/// creating and updating quickening entries for bytecode that can never
/// specialize.
pub(super) fn dense_quickening_candidate_opcode(opcode: DenseOpcode) -> bool {
    matches!(
        opcode,
        DenseOpcode::BinaryAdd
            | DenseOpcode::BinarySub
            | DenseOpcode::BinaryMul
            | DenseOpcode::BinaryConcat
            | DenseOpcode::BinaryConcatEcho
            | DenseOpcode::JumpIfFalse
            | DenseOpcode::JumpIfTrue
            | DenseOpcode::JumpIf
    )
}

/// True when a function body can observe its call-argument vector: a
/// direct call to a func_get_args-style builtin, or any dynamic dispatch
/// or include/eval that could reach one. Bodies that cannot observe the
/// vector let calls skip the per-call argument snapshot entirely
/// (backtraces read the separately kept trace arguments).
pub(super) fn function_body_observes_argument_vector(function: &IrFunction) -> bool {
    function.blocks.iter().any(|block| {
        block
            .instructions
            .iter()
            .any(|instruction| match &instruction.kind {
                InstructionKind::CallFunction { name, .. } => matches!(
                    normalize_function_name(name).as_str(),
                    "func_get_args" | "func_num_args" | "func_get_arg"
                ),
                InstructionKind::CallCallable { .. }
                | InstructionKind::Pipe { .. }
                | InstructionKind::AcquireCallable { .. }
                | InstructionKind::ResolveCallable { .. }
                | InstructionKind::Include { .. }
                | InstructionKind::Eval { .. } => true,
                _ => false,
            })
    })
}

/// Inline plan for a trivial method body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum TrivialMethodPlan {
    /// `return $this->prop;`
    Getter { property: String },
    /// `$this->prop = $x;` optionally returning `$this`.
    Setter {
        property: String,
        returns_this: bool,
    },
}

/// Classifies bodies that are exactly a declared-property read or write on
/// `$this`, with plain positional untyped parameters and no control flow.
/// Everything else stays on generic dispatch.
pub(super) fn classify_trivial_method(function: &IrFunction) -> Option<TrivialMethodPlan> {
    if !function.flags.is_method
        || function.flags.is_static
        || function.flags.is_generator
        || function.flags.is_closure
        || function.returns_by_ref
        || function.return_type.is_some()
        || !function.captures.is_empty()
        || function.blocks.len() != 1
        || function
            .params
            .iter()
            .any(|param| param.by_ref || param.variadic || param.type_.is_some())
    {
        return None;
    }
    let block = function.blocks.first()?;
    let terminator = block.terminator.as_ref()?;
    let instructions = &block.instructions;
    let this_local = LocalId::new(0);
    match (function.params.len(), instructions.as_slice()) {
        // LoadLocal $this; FetchProperty; return value
        (0, [load_this, fetch]) => {
            let InstructionKind::LoadLocal {
                dst: this_reg,
                local,
            } = &load_this.kind
            else {
                return None;
            };
            if *local != this_local {
                return None;
            }
            let InstructionKind::FetchProperty {
                dst: value_reg,
                object: Operand::Register(object_reg),
                property,
            } = &fetch.kind
            else {
                return None;
            };
            if object_reg != this_reg {
                return None;
            }
            let TerminatorKind::Return {
                value: Some(Operand::Register(returned)),
                by_ref_local: None,
            } = &terminator.kind
            else {
                return None;
            };
            (returned == value_reg).then(|| TrivialMethodPlan::Getter {
                property: property.clone(),
            })
        }
        // LoadLocal $this; LoadLocal $x; AssignProperty; Discard
        // [; LoadLocal $this] ; return [$this]
        (1, [load_this, load_param, assign, discard, rest @ ..]) => {
            let InstructionKind::LoadLocal {
                dst: this_reg,
                local,
            } = &load_this.kind
            else {
                return None;
            };
            if *local != this_local {
                return None;
            }
            let InstructionKind::LoadLocal {
                dst: param_reg,
                local: param_local,
            } = &load_param.kind
            else {
                return None;
            };
            if *param_local != LocalId::new(1) {
                return None;
            }
            let InstructionKind::AssignProperty {
                dst: assign_reg,
                object: Operand::Register(object_reg),
                property,
                value: Operand::Register(value_reg),
            } = &assign.kind
            else {
                return None;
            };
            if object_reg != this_reg || value_reg != param_reg {
                return None;
            }
            let InstructionKind::Discard {
                src: Operand::Register(discarded),
            } = &discard.kind
            else {
                return None;
            };
            if discarded != assign_reg {
                return None;
            }
            match (rest, &terminator.kind) {
                (
                    [],
                    TerminatorKind::Return {
                        value: None,
                        by_ref_local: None,
                    },
                ) => Some(TrivialMethodPlan::Setter {
                    property: property.clone(),
                    returns_this: false,
                }),
                (
                    [load_this_again],
                    TerminatorKind::Return {
                        value: Some(Operand::Register(returned)),
                        by_ref_local: None,
                    },
                ) => {
                    let InstructionKind::LoadLocal {
                        dst: this_again_reg,
                        local: this_again_local,
                    } = &load_this_again.kind
                    else {
                        return None;
                    };
                    (*this_again_local == this_local && returned == this_again_reg).then(|| {
                        TrivialMethodPlan::Setter {
                            property: property.clone(),
                            returns_this: true,
                        }
                    })
                }
                _ => None,
            }
        }
        _ => None,
    }
}

pub(super) fn fetch_dim_value(array: &Value, key: &ArrayKey) -> Result<Option<Value>, String> {
    if let Value::Reference(cell) = array {
        return fetch_dim_value(&cell.borrow(), key);
    }
    if let Value::Object(object) = array
        && spl_runtime_marker(object).is_some_and(|class| is_spl_caching_iterator_class(&class))
    {
        spl_caching_iterator_require_full_cache(object, &object.display_name())?;
        return spl_caching_iterator_offset_get(object, &array_key_to_value(key.clone())).map(Some);
    }
    if let Value::Object(object) = array
        && spl_runtime_marker(object).is_some_and(|class| is_spl_array_access_runtime_class(&class))
    {
        return spl_container_offset_get(object, &array_key_to_value(key.clone())).map(Some);
    }
    if let Value::String(string) = array {
        return Ok(string_offset_byte(string, key));
    }
    let Value::Array(array) = array else {
        return Err("E_PHP_VM_ARRAY_FETCH_TYPE: value is not an array".to_owned());
    };
    Ok(array.get(key).map(effective_value))
}

pub(super) fn quiet_dim_fetch_scalar_returns_null(value: &Value) -> bool {
    matches!(
        value,
        Value::Null
            | Value::Bool(_)
            | Value::Int(_)
            | Value::Float(_)
            | Value::Uninitialized
            | Value::Resource(_)
    )
}

pub(super) fn effective_value(value: &Value) -> Value {
    match value {
        Value::Reference(cell) => {
            let _source = layout_source::enter_default(layout_source::REFERENCE_DEREFERENCE);
            cell.get()
        }
        value => {
            let _source = layout_source::enter_default(layout_source::STACK_REGISTER_LOCAL_MOVE);
            value.clone()
        }
    }
}

pub(super) fn effective_is_null_or_false(value: &Value) -> bool {
    match value {
        Value::Reference(cell) => effective_is_null_or_false(&cell.borrow()),
        Value::Null | Value::Bool(false) => true,
        _ => false,
    }
}

pub(super) fn effective_is_uninitialized_or_null(value: &Value) -> bool {
    match value {
        Value::Reference(cell) => effective_is_uninitialized_or_null(&cell.borrow()),
        Value::Uninitialized | Value::Null => true,
        _ => false,
    }
}

pub(super) fn effective_is_array(value: &Value) -> bool {
    match value {
        Value::Reference(cell) => effective_is_array(&cell.borrow()),
        Value::Array(_) => true,
        _ => false,
    }
}

pub(super) fn curl_callback_is_enabled(value: &Value) -> bool {
    !effective_is_null_or_false(value)
}

pub(super) fn collect_compact_variable_names(value: &Value, names: &mut Vec<String>) {
    match effective_value(value) {
        Value::Array(array) => {
            for (_, element) in array.iter() {
                collect_compact_variable_names(element, names);
            }
        }
        Value::String(name) if !name.is_empty() => names.push(name.to_string_lossy()),
        value => {
            if let Ok(name) = to_string(&value)
                && !name.is_empty()
            {
                names.push(name.to_string_lossy());
            }
        }
    }
}

pub(super) fn cast_value_to_object(value: &Value) -> Value {
    match effective_value(value) {
        Value::Object(object) => Value::Object(object),
        Value::Null | Value::Uninitialized => Value::Object(ObjectRef::new_with_display_name(
            &std_class_entry(),
            "stdClass",
        )),
        Value::Array(array) => {
            let object = ObjectRef::new_with_display_name(&std_class_entry(), "stdClass");
            for (key, element) in array.iter() {
                object.set_property(object_cast_property_name(&key), effective_value(element));
            }
            Value::Object(object)
        }
        scalar => {
            let object = ObjectRef::new_with_display_name(&std_class_entry(), "stdClass");
            object.set_property("scalar", scalar);
            Value::Object(object)
        }
    }
}

pub(super) fn cast_value_to_array(
    compiled: &CompiledUnit,
    stack: &CallStack,
    value: &Value,
) -> Value {
    match effective_value(value) {
        Value::Array(array) => Value::Array(array),
        Value::Null | Value::Uninitialized => Value::Array(PhpArray::new()),
        Value::Object(object) => Value::Array(object_vars_array(compiled, stack, &object, true)),
        Value::Callable(callable) if matches!(callable.as_ref(), CallableValue::Closure(_)) => {
            Value::packed_array(vec![Value::Callable(callable)])
        }
        scalar => Value::packed_array(vec![scalar]),
    }
}

pub(super) fn object_cast_property_name(key: &ArrayKey) -> String {
    match key {
        ArrayKey::Int(value) => value.to_string(),
        ArrayKey::String(value) => value.to_string_lossy(),
    }
}

/// Borrowed mirror of [`fetch_dim_path_value`] for read-only predicates.
///
/// Walks `value[dims...]` by reference and applies `f` to the leaf (`None`
/// when a dimension is missing, mirroring `fetch_dim_path_value`'s `Ok(None)`
/// results). Returns `None` — caller must use the cloning path — only for
/// shapes whose reads have side effects or interior borrows the borrowed walk
/// cannot model: SimpleXML dimension access and reference cells that are
/// currently mutably borrowed. Cloning containers for mere isset/empty
/// probes is what shares array handles and forces copy-on-write separations
/// on the next write, so hot registry checks must stay on this path.
pub(super) fn with_borrowed_dim_path<R>(
    value: &Value,
    dims: &[ArrayKey],
    f: &mut dyn FnMut(Option<&Value>) -> R,
) -> Option<R> {
    match value {
        Value::Reference(cell) => cell
            .try_with_value(|inner| with_borrowed_dim_path(inner, dims, f))
            .ok()
            .flatten(),
        _ => {
            let Some((first, rest)) = dims.split_first() else {
                return Some(f(Some(value)));
            };
            match value {
                Value::Array(array) => match array.get(first) {
                    Some(child) => with_borrowed_dim_path(child, rest, f),
                    None => Some(f(None)),
                },
                Value::String(string) => match string_offset_byte(string, first) {
                    Some(byte) => with_borrowed_dim_path(&byte, rest, f),
                    None => Some(f(None)),
                },
                Value::Object(object) if is_simplexml_object(object) => None,
                _ => Some(f(None)),
            }
        }
    }
}

pub(super) fn fetch_dim_path_value(
    value: &Value,
    dims: &[ArrayKey],
) -> Result<Option<Value>, String> {
    let mut current = effective_value(value);
    for key in dims {
        match &current {
            Value::Array(array) => {
                let Some(next) = array.get(key) else {
                    return Ok(None);
                };
                current = effective_value(next);
            }
            Value::String(string) => {
                let Some(next) = string_offset_byte(string, key) else {
                    return Ok(None);
                };
                current = next;
            }
            Value::Object(object) if is_simplexml_object(object) => {
                let next = php_runtime::api::xml::simplexml_dimension(object, key);
                if matches!(next, Value::Null) {
                    return Ok(None);
                }
                current = effective_value(&next);
            }
            _ => return Ok(None),
        }
    }
    Ok(Some(current))
}

pub(super) fn spl_array_access_dim_target(
    value: &Value,
    dims: &[ArrayKey],
) -> Option<(ObjectRef, Value)> {
    let [key] = dims else {
        return None;
    };
    let Value::Object(object) = effective_value(value) else {
        return None;
    };
    spl_runtime_marker(&object)
        .is_some_and(|class| is_spl_array_access_runtime_class(&class))
        .then(|| (object, array_key_to_value(key.clone())))
}

pub(super) fn read_dim_operands_at_frame(
    unit: &IrUnit,
    stack: &CallStack,
    frame_index: usize,
    dims: &[Operand],
) -> Result<Vec<ArrayKey>, String> {
    let values = read_dim_operand_values_at_frame(unit, stack, frame_index, dims)?;
    dim_values_to_array_keys(&values)
}

pub(super) fn read_dim_operand_values_at_frame(
    unit: &IrUnit,
    stack: &CallStack,
    frame_index: usize,
    dims: &[Operand],
) -> Result<Vec<Value>, String> {
    dims.iter()
        .map(|operand| read_operand_at_frame(unit, stack, frame_index, *operand))
        .collect()
}

pub(super) fn dim_values_to_array_keys(values: &[Value]) -> Result<Vec<ArrayKey>, String> {
    values.iter().map(array_key_from_value).collect()
}

pub(super) fn spl_object_storage_local_object(
    stack: &CallStack,
    local: LocalId,
) -> Option<ObjectRef> {
    let Value::Object(object) = effective_value(&read_local_value(stack, local)?) else {
        return None;
    };
    (normalize_class_name(&object.class_name()) == "splobjectstorage").then_some(object)
}

pub(super) fn spl_object_storage_local_object_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> Option<ObjectRef> {
    let Value::Object(object) =
        effective_value(&read_local_value_at_frame(stack, frame_index, local)?)
    else {
        return None;
    };
    (normalize_class_name(&object.class_name()) == "splobjectstorage").then_some(object)
}

pub(super) fn spl_multiple_iterator_local_object(
    stack: &CallStack,
    local: LocalId,
) -> Option<ObjectRef> {
    let Value::Object(object) = effective_value(&read_local_value(stack, local)?) else {
        return None;
    };
    (spl_runtime_marker(&object).as_deref() == Some("multipleiterator")).then_some(object)
}

pub(super) fn spl_multiple_iterator_local_object_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> Option<ObjectRef> {
    let Value::Object(object) =
        effective_value(&read_local_value_at_frame(stack, frame_index, local)?)
    else {
        return None;
    };
    (spl_runtime_marker(&object).as_deref() == Some("multipleiterator")).then_some(object)
}

pub(super) fn spl_array_access_local_object_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> Option<ObjectRef> {
    let Value::Object(object) =
        effective_value(&read_local_value_at_frame(stack, frame_index, local)?)
    else {
        return None;
    };
    spl_runtime_marker(&object)
        .is_some_and(|class| is_spl_array_access_runtime_class(&class))
        .then_some(object)
}

pub(super) fn read_local_value(stack: &CallStack, local: LocalId) -> Option<Value> {
    stack.current()?.locals.get(local)
}

pub(super) fn read_local_value_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> Option<Value> {
    stack.frames().get(frame_index)?.locals.get(local)
}

pub(super) fn local_slot_is_in_bounds(stack: &CallStack, local: LocalId) -> bool {
    stack
        .current()
        .is_some_and(|frame| frame.locals.contains(local))
}

pub(super) fn local_slot_is_in_bounds_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> bool {
    stack
        .frames()
        .get(frame_index)
        .is_some_and(|frame| frame.locals.contains(local))
}

pub(super) fn local_alias_state(stack: &CallStack, local: LocalId) -> AliasState {
    stack
        .current()
        .and_then(|frame| frame.locals.get_slot(local))
        .map(slot_alias_state)
        .unwrap_or(AliasState::UnknownAliasing)
}

pub(super) fn local_alias_state_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> AliasState {
    stack
        .frames()
        .get(frame_index)
        .and_then(|frame| frame.locals.get_slot(local))
        .map(slot_alias_state)
        .unwrap_or(AliasState::UnknownAliasing)
}

pub(super) fn local_array_is_packed_fast(stack: &CallStack, local: LocalId) -> bool {
    stack
        .current()
        .and_then(|frame| frame.locals.get_slot(local))
        .is_some_and(slot_effective_array_is_packed_fast)
}

pub(super) fn local_array_is_packed_fast_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> bool {
    stack
        .frames()
        .get(frame_index)
        .and_then(|frame| frame.locals.get_slot(local))
        .is_some_and(slot_effective_array_is_packed_fast)
}

/// Checks packed-array layout through a local slot without cloning its array
/// handle. The prior read/effective-value path cloned ordinary arrays twice
/// around every dimension write merely to inspect their layout.
pub(super) fn slot_effective_array_is_packed_fast(slot: &Slot) -> bool {
    fn value_is_packed_after_one_deref(value: &Value) -> bool {
        match value {
            Value::Array(array) => array.is_packed_fast(),
            Value::Reference(cell) => cell
                .try_with_value(
                    |value| matches!(value, Value::Array(array) if array.is_packed_fast()),
                )
                .unwrap_or(false),
            _ => false,
        }
    }

    match slot {
        Slot::Value(value) => value_is_packed_after_one_deref(value),
        Slot::Reference(cell) => cell
            .try_with_value(value_is_packed_after_one_deref)
            .unwrap_or(false),
    }
}

/// Returns a local's effective object handle without cloning non-object values.
/// Array-dimension writes use this to probe for userland `ArrayAccess`; normal
/// arrays and scalars stay borrowed and allocate no transient value handle.
pub(super) fn local_effective_object(stack: &CallStack, local: LocalId) -> Option<ObjectRef> {
    fn object_after_one_deref(value: &Value) -> Option<ObjectRef> {
        match value {
            Value::Object(object) => Some(object.clone()),
            Value::Reference(cell) => cell
                .try_with_value(|value| match value {
                    Value::Object(object) => Some(object.clone()),
                    _ => None,
                })
                .ok()
                .flatten(),
            _ => None,
        }
    }

    match stack.current()?.locals.get_slot(local)? {
        Slot::Value(value) => object_after_one_deref(value),
        Slot::Reference(cell) => cell.try_with_value(object_after_one_deref).ok().flatten(),
    }
}

pub(super) fn local_array_has_cow_or_reference_fallback(stack: &CallStack, local: LocalId) -> bool {
    let Some(slot) = stack
        .current()
        .and_then(|frame| frame.locals.get_slot(local))
    else {
        return false;
    };
    match slot {
        Slot::Value(Value::Array(array)) => array.is_shared() || array.contains_references_fast(),
        Slot::Reference(cell) => cell
            .try_with_value(|value| match value {
                Value::Array(array) => array.is_shared() || array.contains_references_fast(),
                _ => true,
            })
            .unwrap_or(true),
        _ => false,
    }
}

pub(super) fn local_array_has_cow_or_reference_fallback_at_frame(
    stack: &CallStack,
    frame_index: usize,
    local: LocalId,
) -> bool {
    let Some(slot) = stack
        .frames()
        .get(frame_index)
        .and_then(|frame| frame.locals.get_slot(local))
    else {
        return false;
    };
    match slot {
        Slot::Value(Value::Array(array)) => array.is_shared() || array.contains_references_fast(),
        Slot::Reference(cell) => cell
            .try_with_value(|value| match value {
                Value::Array(array) => array.is_shared() || array.contains_references_fast(),
                _ => true,
            })
            .unwrap_or(true),
        _ => false,
    }
}

pub(super) fn is_this_local(function: &IrFunction, local: LocalId) -> bool {
    function
        .locals
        .get(local.index())
        .is_some_and(|name| name == "this")
}

pub(super) fn is_globals_local(function: &IrFunction, local: LocalId) -> bool {
    function
        .locals
        .get(local.index())
        .is_some_and(|name| name == "GLOBALS")
}

pub(super) enum ExactEchoBatchPart {
    Bytes(Vec<u8>),
    Empty,
}

pub(super) fn exact_echo_batch_part(value: &Value) -> Option<ExactEchoBatchPart> {
    match value {
        Value::String(value) => Some(ExactEchoBatchPart::Bytes(value.as_bytes().to_vec())),
        Value::Int(value) => Some(ExactEchoBatchPart::Bytes(value.to_string().into_bytes())),
        Value::Bool(true) => Some(ExactEchoBatchPart::Bytes(b"1".to_vec())),
        Value::Bool(false) | Value::Null => Some(ExactEchoBatchPart::Empty),
        Value::Float(_)
        | Value::Array(_)
        | Value::Object(_)
        | Value::Resource(_)
        | Value::Reference(_)
        | Value::Callable(_)
        | Value::Fiber(_)
        | Value::Generator(_)
        | Value::Uninitialized => None,
    }
}

pub(super) fn collect_exact_echo_batch_at_frame(
    vm: &Vm,
    unit: &IrUnit,
    stack: &CallStack,
    frame_index: usize,
    instructions: &[Instruction],
    instruction_index: usize,
    first_value: &Value,
) -> Option<(Vec<ExactEchoBatchPart>, usize)> {
    let mut parts = vec![exact_echo_batch_part(first_value)?];
    let mut next_index = instruction_index + 1;
    while let Some(instruction) = instructions.get(next_index) {
        match &instruction.kind {
            InstructionKind::Echo { src } => {
                let Ok(value) = read_operand_ref_at_frame(unit, stack, frame_index, *src) else {
                    break;
                };
                let Some(part) = exact_echo_batch_part(value.as_value()) else {
                    break;
                };
                parts.push(part);
                next_index += 1;
            }
            InstructionKind::LoadConst { dst, constant } => {
                let Some(next) = instructions.get(next_index + 1) else {
                    break;
                };
                let InstructionKind::Echo { src } = &next.kind else {
                    break;
                };
                if !matches!(src, Operand::Register(register) if *register == *dst) {
                    break;
                }
                let Ok(value) = vm.constant_value(unit, *constant) else {
                    break;
                };
                let Some(part) = exact_echo_batch_part(&value) else {
                    break;
                };
                parts.push(part);
                next_index += 2;
            }
            _ => break,
        }
    }
    Some((parts, next_index))
}

pub(super) fn write_exact_echo_batch(output: &mut OutputBuffer, parts: &[ExactEchoBatchPart]) {
    let slices = parts
        .iter()
        .filter_map(|part| match part {
            ExactEchoBatchPart::Bytes(bytes) if !bytes.is_empty() => Some(bytes.as_slice()),
            ExactEchoBatchPart::Bytes(_) | ExactEchoBatchPart::Empty => None,
        })
        .collect::<Vec<_>>();
    output.write_fast_slices(&slices);
}

#[cold]
pub(super) fn concat_fallback_reason(lhs: &Value, rhs: &Value) -> Option<&'static str> {
    concat_operand_fallback_reason(lhs).or_else(|| concat_operand_fallback_reason(rhs))
}

#[cold]
pub(super) fn concat_operand_fallback_reason(value: &Value) -> Option<&'static str> {
    match value {
        Value::String(_) => None,
        Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) => Some("scalar_conversion"),
        Value::Array(_) => Some("array_conversion_warning"),
        Value::Object(_) | Value::Fiber(_) | Value::Generator(_) => Some("object_to_string"),
        Value::Resource(_) => Some("resource_conversion"),
        Value::Reference(_) => Some("reference_deref"),
        Value::Callable(_) => Some("callable_conversion_error"),
        Value::Uninitialized => Some("uninitialized_conversion_error"),
    }
}
