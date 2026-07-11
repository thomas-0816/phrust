use super::*;

pub(super) fn emit_spl_array_access_bind_reference_notice(
    compiled: &CompiledUnit,
    output: &mut OutputBuffer,
    stack: &CallStack,
    state: &mut ExecutionState,
    object: &ObjectRef,
    span: php_ir::IrSpan,
) {
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_VM_ARRAY_ACCESS_BIND_REFERENCE_NOTICE",
        RuntimeSeverity::Notice,
        format!(
            "Indirect modification of overloaded element of {} has no effect",
            object.display_name()
        ),
        runtime_source_span(compiled, span),
        stack_trace(compiled, stack),
        Some(php_runtime::PhpReferenceClassification::Warning),
    );
    if error_reporting_allows(state, php_runtime::PHP_E_NOTICE) {
        let leading_newline = !output.as_bytes().is_empty();
        emit_vm_diagnostic_with_options(
            output,
            state,
            &diagnostic,
            php_runtime::PhpDiagnosticChannel::Notice,
            php_runtime::PHP_E_NOTICE,
            leading_newline,
        );
        state.diagnostics.push(diagnostic);
    }
}

pub(super) fn emit_static_property_as_non_static_notice(
    compiled: &CompiledUnit,
    output: &mut OutputBuffer,
    stack: &CallStack,
    state: &mut ExecutionState,
    class: &php_ir::module::ClassEntry,
    property: &php_ir::module::ClassPropertyEntry,
    span: php_ir::IrSpan,
) {
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_VM_STATIC_PROPERTY_AS_NON_STATIC_NOTICE",
        RuntimeSeverity::Notice,
        format!(
            "Accessing static property {}::${} as non static",
            class.display_name, property.name
        ),
        runtime_source_span(compiled, span),
        stack_trace(compiled, stack),
        Some(php_runtime::PhpReferenceClassification::Warning),
    );
    if error_reporting_allows(state, php_runtime::PHP_E_NOTICE) {
        emit_vm_diagnostic(
            output,
            state,
            &diagnostic,
            php_runtime::PhpDiagnosticChannel::Notice,
            php_runtime::PHP_E_NOTICE,
        );
        state.diagnostics.push(diagnostic);
    }
}

/// How `assign_dim_local` reached the container, for slot-fast counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AssignDimLocalPath {
    /// Mutated the slot (or its reference cell) in place — no transient
    /// handle clone, so copy-on-write only separates for real sharing.
    InPlace,
    /// The reference cell was already borrowed; used the clone-based
    /// read/mutate/write-back path.
    ClonedReferenceFallback,
}

pub(super) fn assign_dim_local(
    stack: &mut CallStack,
    local: LocalId,
    dims: &[ArrayKey],
    value: Value,
    append: bool,
) -> Result<AssignDimLocalPath, String> {
    let frame = stack.current_mut().ok_or("no active frame")?;
    let Some(slot) = frame.locals.get_slot_mut(local) else {
        return Err(format!("invalid local local:{}", local.raw()));
    };
    let mut pending = Some(value);
    let in_place = slot.try_with_effective_value_mut(|current| {
        if matches!(current, Value::Uninitialized | Value::Null) {
            *current = Value::Array(PhpArray::new());
        }
        assign_dim_value(
            current,
            dims,
            pending.take().expect("assign value consumed once"),
            append,
        )
    });
    if let Some(result) = in_place {
        return result.map(|()| AssignDimLocalPath::InPlace);
    }
    let mut current = slot.read();
    if matches!(current, Value::Uninitialized | Value::Null) {
        current = Value::Array(PhpArray::new());
    }
    assign_dim_value(
        &mut current,
        dims,
        pending.take().expect("assign value still pending"),
        append,
    )?;
    slot.write(current);
    Ok(AssignDimLocalPath::ClonedReferenceFallback)
}

pub(super) fn assign_globals_dim(
    globals: &mut GlobalSymbolTable,
    dims: &[ArrayKey],
    value: Value,
    append: bool,
) -> Result<(), String> {
    if append {
        return Err(
            "E_PHP_VM_GLOBALS_APPEND_GAP: appending directly to $GLOBALS is not implemented"
                .to_owned(),
        );
    }
    let Some((first, rest)) = dims.split_first() else {
        return Err("E_PHP_VM_GLOBALS_ASSIGN_DIM: missing $GLOBALS key".to_owned());
    };
    let ArrayKey::String(name) = first else {
        return Err(
            "E_PHP_VM_GLOBALS_ASSIGN_KEY: $GLOBALS keys must be strings in runtime-semantics"
                .to_owned(),
        );
    };
    let name = name.to_string();
    if rest.is_empty() {
        globals.set(name, value);
        return Ok(());
    }
    let cell = globals.ensure_slot(name, Value::Array(PhpArray::new()));
    let mut current = cell.get();
    if matches!(current, Value::Uninitialized | Value::Null) {
        current = Value::Array(PhpArray::new());
    }
    assign_dim_value(&mut current, rest, value, false)?;
    cell.set(current);
    Ok(())
}

pub(super) fn unset_globals_dim(
    globals: &mut GlobalSymbolTable,
    dims: &[ArrayKey],
) -> Result<(), String> {
    let Some((first, rest)) = dims.split_first() else {
        return Ok(());
    };
    let ArrayKey::String(name) = first else {
        return Ok(());
    };
    let Some(cell) = globals.get_slot(&name.to_string()) else {
        return Ok(());
    };
    if rest.is_empty() {
        cell.set(Value::Uninitialized);
        return Ok(());
    }
    let mut current = cell.get();
    unset_dim_value(&mut current, rest);
    cell.set(current);
    Ok(())
}

/// Write a single byte into a string offset, following PHP semantics: the first
/// byte of the value replaces the byte at the index, the string is padded with
/// spaces when the index is past the end, and only a single integer dimension is
/// allowed.
pub(super) fn write_string_offset(
    mut bytes: Vec<u8>,
    dims: &[ArrayKey],
    value: Value,
    append: bool,
) -> Result<Vec<u8>, String> {
    if append {
        return Err("E_PHP_VM_STRING_APPEND: [] operator not supported for strings".to_owned());
    }
    let [key] = dims else {
        return Err(
            "E_PHP_VM_STRING_OFFSET_NESTED: cannot use a nested write on a string offset"
                .to_owned(),
        );
    };
    let index = match key {
        ArrayKey::Int(value) => *value,
        ArrayKey::String(value) => leading_int_offset(value.as_bytes()).ok_or_else(|| {
            "E_PHP_VM_STRING_OFFSET_TYPE: Cannot access offset of type string on string".to_owned()
        })?,
    };
    let length = bytes.len() as i64;
    let resolved = if index < 0 { index + length } else { index };
    if resolved < 0 {
        return Err(format!(
            "E_PHP_VM_STRING_OFFSET_NEGATIVE: Illegal string offset {index}"
        ));
    }
    let replacement =
        to_string(&value).map_err(|message| format!("E_PHP_VM_STRING_OFFSET_VALUE: {message}"))?;
    let Some(&first) = replacement.as_bytes().first() else {
        return Err(
            "E_PHP_VM_STRING_OFFSET_EMPTY: Cannot assign an empty string to a string offset"
                .to_owned(),
        );
    };
    let index = resolved as usize;
    if index >= bytes.len() {
        bytes.resize(index + 1, b' ');
    }
    bytes[index] = first;
    Ok(bytes)
}

/// Outcome of attempting an in-place property dimension assignment.
pub(super) enum PropertyDimInPlace {
    /// The property held an array and `assign_dim_value` ran directly on
    /// the stored value; carries that call's result.
    Applied(Result<(), String>),
    /// The property is missing, holds a non-array value, or the object
    /// storage is unavailable; the caller must run the generic
    /// read-clone → assign → write-back path.
    NotEligible,
}

/// Assigns `object->property[dims...] = value` directly on the stored
/// property value when it currently holds an array. This avoids the generic
/// path's property read (which shares the array handle) followed by
/// `assign_dim_value` on the copy (which then deep-copies the entire array
/// storage through a COW separation) on every nested write.
///
/// Callers gate on property shape first: untyped, non-readonly,
/// non-hooked properties only, with visibility already validated. Non-array
/// slot values (objects, references, strings, scalars, null) report
/// `NotEligible` so ArrayAccess dispatch, string offsets, vivification and
/// error behavior stay on the generic path unchanged.
pub(super) fn try_assign_property_dim_in_place(
    object: &ObjectRef,
    storage_name: &str,
    dims: &[ArrayKey],
    value: Value,
    append: bool,
) -> PropertyDimInPlace {
    let mut pending = Some(value);
    match object.try_modify_property_value(storage_name, |slot| {
        if !matches!(slot, Value::Array(_)) {
            return None;
        }
        Some(assign_dim_value(
            slot,
            dims,
            pending.take().expect("assign value consumed once"),
            append,
        ))
    }) {
        Ok(Some(Some(result))) => PropertyDimInPlace::Applied(result),
        Ok(Some(None)) | Ok(None) | Err(_) => PropertyDimInPlace::NotEligible,
    }
}

pub(super) fn assign_dim_value(
    container: &mut Value,
    dims: &[ArrayKey],
    value: Value,
    append: bool,
) -> Result<(), String> {
    if let Value::Reference(cell) = container {
        let mut pending = Some(value);
        if let Ok(result) = cell.try_with_value_mut(|current| {
            assign_dim_value(
                current,
                dims,
                pending.take().expect("assign value consumed once"),
                append,
            )
        }) {
            return result;
        }
        let mut current = cell.get();
        assign_dim_value(
            &mut current,
            dims,
            pending.take().expect("assign value still pending"),
            append,
        )?;
        cell.set(current);
        return Ok(());
    }
    if let Value::Object(object) = container
        && spl_runtime_marker(object).is_some_and(|class| is_spl_caching_iterator_class(&class))
    {
        if dims.len() > 1 {
            return Err(
                "E_PHP_VM_SPL_CONTAINER_NESTED_DIM: nested ArrayAccess writes are not implemented"
                    .to_owned(),
            );
        }
        let key = if append {
            Value::Null
        } else {
            let Some(key) = dims.first() else {
                return Err("E_PHP_VM_ARRAY_ASSIGN_DIM: missing array dimension".to_owned());
            };
            array_key_to_value(key.clone())
        };
        spl_caching_iterator_require_full_cache(object, &object.display_name())?;
        spl_caching_iterator_offset_set(object, &key, value)?;
        return Ok(());
    }
    if let Value::Object(object) = container
        && spl_runtime_marker(object).is_some_and(|class| is_spl_array_access_runtime_class(&class))
    {
        if dims.len() > 1 {
            return Err(
                "E_PHP_VM_SPL_CONTAINER_NESTED_DIM: nested ArrayAccess writes are not implemented"
                    .to_owned(),
            );
        }
        let key = if append {
            Value::Null
        } else {
            let Some(key) = dims.first() else {
                return Err("E_PHP_VM_ARRAY_ASSIGN_DIM: missing array dimension".to_owned());
            };
            array_key_to_value(key.clone())
        };
        spl_container_offset_set(object, key, value)?;
        return Ok(());
    }
    if let Value::String(string) = container {
        let updated = write_string_offset(string.as_bytes().to_vec(), dims, value, append)?;
        *container = Value::string(updated);
        return Ok(());
    }
    let Value::Array(array) = container else {
        return Err(format!(
            "E_PHP_VM_ARRAY_ASSIGN_TYPE: cannot assign dimension on {}",
            value_type_name(container)
        ));
    };
    let Some((first, rest)) = dims.split_first() else {
        if append {
            array.append(value);
            return Ok(());
        }
        return Err("E_PHP_VM_ARRAY_ASSIGN_DIM: missing array dimension".to_owned());
    };
    if rest.is_empty() && !append {
        if let Some(mut existing) = array.get_mut(first) {
            write_lvalue(&mut existing, value);
        } else {
            array.insert(first.clone(), value);
        }
        return Ok(());
    }
    if array.get(first).is_none() {
        array.insert(first.clone(), Value::Array(PhpArray::new()));
    }
    let Some(mut child) = array.get_mut(first) else {
        return Err("E_PHP_VM_ARRAY_ASSIGN_DIM: failed to create nested array".to_owned());
    };
    if matches!(*child, Value::Uninitialized | Value::Null) {
        *child = Value::Array(PhpArray::new());
    }
    assign_dim_value(&mut child, rest, value, append)
}

pub(super) fn bind_dim_local_to_reference_cell(
    stack: &mut CallStack,
    local: LocalId,
    dims: &[ArrayKey],
    append: bool,
    cell: ReferenceCell,
) -> Result<(), String> {
    let frame = stack.current_mut().ok_or("no active frame")?;
    let Some(slot) = frame.locals.get_slot_mut(local) else {
        return Err(format!("invalid local local:{}", local.raw()));
    };
    let mut current = slot.read();
    if matches!(current, Value::Uninitialized | Value::Null) {
        current = Value::Array(PhpArray::new());
    }
    bind_dim_value_to_reference_cell(&mut current, dims, append, cell)?;
    slot.write(current);
    Ok(())
}

pub(super) fn bind_dim_value_to_reference_cell(
    container: &mut Value,
    dims: &[ArrayKey],
    append: bool,
    cell: ReferenceCell,
) -> Result<(), String> {
    if let Value::Reference(container_cell) = container {
        let mut current = container_cell.get();
        bind_dim_value_to_reference_cell(&mut current, dims, append, cell)?;
        container_cell.set(current);
        return Ok(());
    }
    let Value::Array(array) = container else {
        return Err(format!(
            "E_PHP_VM_ARRAY_BIND_DIM_TYPE: cannot bind dimension on {}",
            value_type_name(container)
        ));
    };
    let Some((first, rest)) = dims.split_first() else {
        if append {
            array.append(Value::Reference(cell));
            return Ok(());
        }
        return Err("E_PHP_VM_ARRAY_BIND_DIM: missing array dimension".to_owned());
    };
    if rest.is_empty() && !append {
        array.insert(first.clone(), Value::Reference(cell));
        return Ok(());
    }
    if array.get(first).is_none() {
        array.insert(first.clone(), Value::Array(PhpArray::new()));
    }
    let Some(mut child) = array.get_mut(first) else {
        return Err("E_PHP_VM_ARRAY_BIND_DIM: failed to create nested array".to_owned());
    };
    if matches!(*child, Value::Uninitialized | Value::Null) {
        *child = Value::Array(PhpArray::new());
    }
    bind_dim_value_to_reference_cell(&mut child, rest, append, cell)
}

pub(super) fn bind_property_dim_to_reference_cell(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    stack: &CallStack,
    object: &ObjectRef,
    property: &str,
    dims: &[ArrayKey],
    append: bool,
    cell: ReferenceCell,
) -> Result<(), String> {
    let storage_name =
        property_dimension_storage_name(compiled, state, stack, object, property, true)?;
    let mut current = object
        .get_property(&storage_name)
        .or_else(|| object.get_property(property))
        .unwrap_or(Value::Null);
    if matches!(current, Value::Uninitialized | Value::Null) {
        current = Value::Array(PhpArray::new());
    }
    bind_dim_value_to_reference_cell(&mut current, dims, append, cell)?;
    object.set_property(storage_name, current);
    Ok(())
}

pub(super) fn ensure_dim_reference_cell(
    stack: &mut CallStack,
    local: LocalId,
    dims: &[ArrayKey],
) -> Result<ReferenceCell, String> {
    let frame = stack.current_mut().ok_or("no active frame")?;
    let Some(slot) = frame.locals.get_slot_mut(local) else {
        return Err(format!("invalid local local:{}", local.raw()));
    };
    let mut current = slot.read();
    if matches!(current, Value::Uninitialized | Value::Null) {
        current = Value::Array(PhpArray::new());
    }
    let cell = ensure_dim_reference_cell_value(&mut current, dims)?;
    slot.write(current);
    Ok(cell)
}

pub(super) fn ensure_dim_reference_cell_value(
    container: &mut Value,
    dims: &[ArrayKey],
) -> Result<ReferenceCell, String> {
    if let Value::Reference(container_cell) = container {
        let mut current = container_cell.get();
        let cell = ensure_dim_reference_cell_value(&mut current, dims)?;
        container_cell.set(current);
        return Ok(cell);
    }
    let Value::Array(array) = container else {
        return Err(format!(
            "E_PHP_VM_ARRAY_REF_DIM_TYPE: cannot reference dimension on {}",
            value_type_name(container)
        ));
    };
    let Some((first, rest)) = dims.split_first() else {
        return Err("E_PHP_VM_ARRAY_REF_DIM: missing array dimension".to_owned());
    };
    if array.get(first).is_none() {
        array.insert(first.clone(), Value::Null);
    }
    let Some(mut child) = array.get_mut(first) else {
        return Err("E_PHP_VM_ARRAY_REF_DIM: failed to create array element".to_owned());
    };
    if rest.is_empty() {
        return Ok(ensure_value_reference_cell(&mut child));
    }
    if matches!(*child, Value::Uninitialized | Value::Null) {
        *child = Value::Array(PhpArray::new());
    }
    ensure_dim_reference_cell_value(&mut child, rest)
}

pub(super) fn ensure_value_reference_cell(value: &mut Value) -> ReferenceCell {
    Lvalue::value(value, LvalueKind::ArrayElement)
        .ensure_reference_cell()
        .expect("array element lvalue can become a reference cell")
}

pub(super) fn write_property_storage_value(object: &ObjectRef, storage_name: &str, value: Value) {
    Lvalue::object_property(object.clone(), storage_name, LvalueKind::ObjectProperty)
        .write_value(value)
        .expect("object property lvalue writes are supported")
}

pub(super) fn write_lvalue(target: &mut Value, value: Value) {
    Lvalue::value(target, LvalueKind::ArrayElement)
        .write_value(value)
        .expect("array element lvalue writes are supported")
}

pub(super) fn unset_dim_local(
    stack: &mut CallStack,
    local: LocalId,
    dims: &[ArrayKey],
) -> Result<(), String> {
    let frame = stack.current_mut().ok_or("no active frame")?;
    let Some(slot) = frame.locals.get_slot_mut(local) else {
        return Err(format!("invalid local local:{}", local.raw()));
    };
    let mut current = slot.read();
    unset_dim_value(&mut current, dims);
    slot.write(current);
    Ok(())
}

pub(super) fn unset_dim_value(container: &mut Value, dims: &[ArrayKey]) {
    if let Value::Reference(cell) = container {
        let mut current = cell.get();
        unset_dim_value(&mut current, dims);
        cell.set(current);
        return;
    }
    let Some((first, rest)) = dims.split_first() else {
        return;
    };
    if let Value::Object(object) = container
        && spl_runtime_marker(object).is_some_and(|class| is_spl_caching_iterator_class(&class))
        && rest.is_empty()
    {
        let _ = spl_caching_iterator_require_full_cache(object, &object.display_name()).and_then(
            |()| spl_caching_iterator_offset_unset(object, &array_key_to_value(first.clone())),
        );
        return;
    }
    if let Value::Object(object) = container
        && spl_runtime_marker(object).is_some_and(|class| is_spl_array_access_runtime_class(&class))
        && rest.is_empty()
    {
        let _ = spl_container_offset_unset(object, &array_key_to_value(first.clone()));
        return;
    }
    let Value::Array(array) = container else {
        return;
    };
    if rest.is_empty() {
        array.remove(first);
        return;
    }
    if let Some(mut child) = array.get_mut(first) {
        unset_dim_value(&mut child, rest);
    }
}

pub(super) fn php_empty(value: &Value) -> Result<bool, String> {
    match value {
        Value::Reference(cell) => php_empty(&cell.borrow()),
        Value::Uninitialized | Value::Null => Ok(true),
        Value::Bool(value) => Ok(!*value),
        Value::Int(value) => Ok(*value == 0),
        Value::Float(value) => {
            let value = value.to_f64();
            Ok(value == 0.0 || value.is_nan())
        }
        Value::String(value) => Ok(value.is_empty() || value.as_bytes() == b"0"),
        Value::Array(array) => Ok(array.is_empty()),
        Value::Object(_)
        | Value::Resource(_)
        | Value::Fiber(_)
        | Value::Generator(_)
        | Value::Callable(_) => Ok(false),
    }
}

pub(super) fn php_empty_access_value(value: &Value) -> Result<bool, String> {
    match effective_value(value) {
        Value::Object(object) if is_simplexml_object(&object) => {
            Ok(php_runtime::xml::simplexml_empty_access(&object))
        }
        value => php_empty(&value),
    }
}

pub(super) fn is_simplexml_object(object: &ObjectRef) -> bool {
    normalize_class_name(&object.class_name()) == "simplexmlelement"
}

pub(super) fn illegal_string_offset_warning(
    key_bytes: &[u8],
    span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    let key = String::from_utf8_lossy(key_bytes);
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_ILLEGAL_STRING_OFFSET",
        RuntimeSeverity::Warning,
        format!("Illegal string offset \"{key}\""),
        span,
        stack_trace,
        Some(php_runtime::PhpReferenceClassification::Warning),
    )
}

pub(super) fn uninitialized_string_offset_warning(
    index: i64,
    span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_UNINITIALIZED_STRING_OFFSET",
        RuntimeSeverity::Warning,
        format!("Uninitialized string offset {index}"),
        span,
        stack_trace,
        Some(php_runtime::PhpReferenceClassification::Warning),
    )
}

pub(super) fn undefined_array_key_warning(
    key: &ArrayKey,
    span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    let key = match key {
        ArrayKey::Int(value) => value.to_string(),
        ArrayKey::String(value) => format!("\"{}\"", value.to_string_lossy()),
    };
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_UNDEFINED_ARRAY_KEY_WARNING",
        RuntimeSeverity::Warning,
        format!("Undefined array key {key}"),
        span,
        stack_trace,
        Some(php_runtime::PhpReferenceClassification::Warning),
    )
}

pub(super) fn undefined_array_string_key_warning(
    key: &PhpString,
    span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_UNDEFINED_ARRAY_KEY_WARNING",
        RuntimeSeverity::Warning,
        format!("Undefined array key \"{}\"", key.to_string_lossy()),
        span,
        stack_trace,
        Some(php_runtime::PhpReferenceClassification::Warning),
    )
}

pub(super) fn array_offset_on_scalar_warning(
    value: &Value,
    span: RuntimeSourceSpan,
    stack_trace: Vec<RuntimeStackFrame>,
) -> RuntimeDiagnostic {
    RuntimeDiagnostic::new(
        "E_PHP_RUNTIME_ARRAY_OFFSET_ON_SCALAR_WARNING",
        RuntimeSeverity::Warning,
        format!(
            "Trying to access array offset on {}",
            array_offset_scalar_type_name(value)
        ),
        span,
        stack_trace,
        Some(php_runtime::PhpReferenceClassification::Warning),
    )
}

pub(super) fn array_offset_scalar_type_name(value: &Value) -> &'static str {
    match value {
        Value::Reference(cell) => array_offset_scalar_type_name(&cell.borrow()),
        Value::Null | Value::Uninitialized => "null",
        Value::Bool(false) => "false",
        Value::Bool(true) => "true",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::Resource(_) => "resource",
        other => value_type_name(other),
    }
}
