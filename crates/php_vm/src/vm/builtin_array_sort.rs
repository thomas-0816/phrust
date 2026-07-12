//! Array sorting argument, reference, and value helpers.

use super::builtin_adapter::builtin_source_span;
use super::prelude::*;
use super::runtime_operations::write_object_numeric_cast_warning;

pub(super) fn is_array_sort_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "array_multisort"
            | "sort"
            | "rsort"
            | "asort"
            | "arsort"
            | "ksort"
            | "krsort"
            | "usort"
            | "uasort"
            | "uksort"
            | "natsort"
            | "natcasesort"
    )
}

pub(super) fn array_callback_key_value(key: &ArrayKey) -> Value {
    match key {
        ArrayKey::Int(index) => Value::Int(*index),
        ArrayKey::String(value) => Value::String(value.clone()),
    }
}

pub(super) fn sort_reference_cell(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    function: &str,
    arg: CallArgument,
    stack: &mut CallStack,
) -> Result<ReferenceCell, ArrayCallbackError> {
    sort_reference_cell_at(compiled, state, function, arg, stack, 1)
}

fn sort_reference_cell_at(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    function: &str,
    arg: CallArgument,
    stack: &mut CallStack,
    position: usize,
) -> Result<ReferenceCell, ArrayCallbackError> {
    if let Some(cell) = call_argument_reference_cell(compiled, Some(state), &arg, stack)
        .map_err(ArrayCallbackError::Message)?
    {
        return Ok(cell);
    }
    match arg.value {
        Value::Reference(cell) => Ok(cell),
        other => Err(ArrayCallbackError::Message(format!(
            "E_PHP_VM_SORT_BY_REF_ARG: {function} argument #{position} must be a mutable array variable, {} given",
            value_type_name(&other)
        ))),
    }
}

pub(super) fn sort_callback_args(
    name: &str,
    left: &(ArrayKey, Value),
    right: &(ArrayKey, Value),
) -> Vec<Value> {
    if name == "uksort" {
        vec![
            array_callback_key_value(&left.0),
            array_callback_key_value(&right.0),
        ]
    } else {
        vec![left.1.clone(), right.1.clone()]
    }
}

pub(super) fn sort_callback_ordering(
    name: &str,
    result: Value,
    reverse: bool,
) -> Result<std::cmp::Ordering, ArrayCallbackError> {
    let int = to_int(&result)
        .map_err(|message| ArrayCallbackError::Message(format!("{name}: {message}")))?;
    let ordering = int.cmp(&0);
    Ok(if reverse {
        ordering.reverse()
    } else {
        ordering
    })
}

pub(super) fn emit_sort_bool_compare_deprecation(
    compiled: &CompiledUnit,
    name: &str,
    output: &mut OutputBuffer,
    stack: &CallStack,
    state: &mut ExecutionState,
    emitted: &mut bool,
) {
    if *emitted {
        return;
    }
    *emitted = true;
    let diagnostic = RuntimeDiagnostic::new(
        "E_PHP_VM_SORT_BOOL_COMPARE_DEPRECATED",
        RuntimeSeverity::Deprecation,
        format!(
            "{name}(): Returning bool from comparison function is deprecated, return an integer less than, equal to, or greater than zero"
        ),
        builtin_source_span(compiled, None),
        stack_trace(compiled, stack),
        None,
    );
    emit_vm_diagnostic(
        output,
        state,
        &diagnostic,
        php_runtime::api::PhpDiagnosticChannel::Deprecated,
        php_runtime::api::PHP_E_DEPRECATED,
    );
    state.diagnostics.push(diagnostic);
}

pub(super) fn multisort_reference_cell_at(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    _function: &str,
    arg: CallArgument,
    stack: &mut CallStack,
    _position: usize,
) -> Result<ReferenceCell, ArrayCallbackError> {
    if let Some(cell) = call_argument_reference_cell(compiled, Some(state), &arg, stack)
        .map_err(ArrayCallbackError::Message)?
    {
        return Ok(cell);
    }
    match arg.value {
        Value::Reference(cell) => Ok(cell),
        other => Ok(ReferenceCell::new(other)),
    }
}

pub(super) fn sort_argument_is_array(
    compiled: &CompiledUnit,
    state: &ExecutionState,
    arg: &CallArgument,
    stack: &mut CallStack,
) -> Result<bool, ArrayCallbackError> {
    if let Some(cell) = call_argument_reference_cell(compiled, Some(state), arg, stack)
        .map_err(ArrayCallbackError::Message)?
    {
        return Ok(effective_is_array(&Value::Reference(cell)));
    }
    Ok(effective_is_array(&arg.value))
}

pub(super) fn multisort_array_entries(
    function: &str,
    position: usize,
    value: &Value,
) -> Result<Vec<(ArrayKey, Value)>, ArrayCallbackError> {
    match value {
        Value::Array(array) => Ok(array
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()),
        Value::Int(flag) if matches!(*flag, SORT_REGULAR | SORT_NUMERIC) => {
            Err(ArrayCallbackError::Message(format!(
                "E_PHP_RUNTIME_BUILTIN_VALUE: {function}(): Argument #{position} ($array) must be an array or a sort flag that has not already been specified"
            )))
        }
        Value::Int(_) => Err(ArrayCallbackError::Message(format!(
            "E_PHP_RUNTIME_BUILTIN_VALUE: {function}(): Argument #{position} ($array) must be a valid sort flag"
        ))),
        _ => Err(ArrayCallbackError::BuiltinTypeMessage(format!(
            "{function}(): Argument #{position} ($array) must be an array or a sort flag"
        ))),
    }
}

pub(super) fn multisort_duplicate_flag_error(
    function: &str,
    position: usize,
) -> ArrayCallbackError {
    ArrayCallbackError::BuiltinTypeMessage(format!(
        "{function}(): Argument #{position} must be an array or a sort flag that has not already been specified"
    ))
}

pub(super) fn sort_numeric_float(
    value: &Value,
    output: &mut OutputBuffer,
    state: &mut ExecutionState,
    source_span: RuntimeSourceSpan,
) -> Result<f64, ArrayCallbackError> {
    match value {
        Value::Reference(cell) => sort_numeric_float(&cell.get(), output, state, source_span),
        Value::Object(object) => {
            write_object_numeric_cast_warning(output, state, object, "float", source_span);
            Ok(1.0)
        }
        other => to_float(other)
            .map_err(|message| ArrayCallbackError::Message(format!("array_multisort: {message}"))),
    }
}

pub(super) fn multisort_numeric_values(
    entries: &[(ArrayKey, Value)],
    output: &mut OutputBuffer,
    state: &mut ExecutionState,
    source_span: RuntimeSourceSpan,
) -> Result<Vec<f64>, ArrayCallbackError> {
    entries
        .iter()
        .map(|(_, value)| multisort_numeric_value(value, output, state, source_span.clone()))
        .collect()
}

fn multisort_numeric_value(
    value: &Value,
    output: &mut OutputBuffer,
    state: &mut ExecutionState,
    source_span: RuntimeSourceSpan,
) -> Result<f64, ArrayCallbackError> {
    match value {
        Value::Reference(cell) => multisort_numeric_value(&cell.get(), output, state, source_span),
        Value::Object(object) => {
            write_object_numeric_cast_warning(output, state, object, "float", source_span.clone());
            write_object_numeric_cast_warning(output, state, object, "float", source_span);
            Ok(1.0)
        }
        other => to_float(other)
            .map_err(|message| ArrayCallbackError::Message(format!("array_multisort: {message}"))),
    }
}

pub(super) fn multisort_reorder_entries(
    entries: &[(ArrayKey, Value)],
    order: &[usize],
) -> PhpArray {
    let mut sorted = PhpArray::new();
    for index in order {
        let (key, value) = &entries[*index];
        match key {
            ArrayKey::String(_) => {
                sorted.insert(key.clone(), value.clone());
            }
            ArrayKey::Int(_) => {
                sorted.append(value.clone());
            }
        }
    }
    sorted
}

pub(super) fn compare_sort_values(
    left: &Value,
    right: &Value,
    flags: i64,
) -> Result<std::cmp::Ordering, String> {
    let normalized = flags & !SORT_FLAG_CASE;
    let case_insensitive = (flags & SORT_FLAG_CASE) != 0;
    match normalized {
        SORT_REGULAR => compare(left, right),
        SORT_NUMERIC => Ok(to_float(left)?
            .partial_cmp(&to_float(right)?)
            .unwrap_or(std::cmp::Ordering::Equal)),
        SORT_STRING | SORT_LOCALE_STRING => {
            let left = sort_string_value(left, case_insensitive);
            let right = sort_string_value(right, case_insensitive);
            Ok(left.cmp(&right))
        }
        SORT_NATURAL => Ok(natural_compare_values(left, right, case_insensitive)),
        _ => compare(left, right),
    }
}

fn natural_compare_values(
    left: &Value,
    right: &Value,
    case_insensitive: bool,
) -> std::cmp::Ordering {
    let left = sort_string_value(left, case_insensitive);
    let right = sort_string_value(right, case_insensitive);
    natural_compare_bytes(left.as_bytes(), right.as_bytes())
}

pub(super) fn sort_string_value(value: &Value, case_insensitive: bool) -> String {
    let text = match to_string(value) {
        Ok(value) => value.to_string_lossy(),
        Err(_) => format!("{value:?}"),
    };
    if case_insensitive {
        text.to_ascii_lowercase()
    } else {
        text
    }
}

pub(super) fn natural_compare_bytes(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    let mut left_index = 0;
    let mut right_index = 0;
    while left_index < left.len() && right_index < right.len() {
        while left_index < left.len() && left[left_index].is_ascii_whitespace() {
            left_index += 1;
        }
        while right_index < right.len() && right[right_index].is_ascii_whitespace() {
            right_index += 1;
        }
        match (left_index == left.len(), right_index == right.len()) {
            (true, true) => return std::cmp::Ordering::Equal,
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            (false, false) => {}
        }
        let left_byte = left[left_index];
        let right_byte = right[right_index];
        if left_byte.is_ascii_digit() && right_byte.is_ascii_digit() {
            let left_start = left_index;
            let right_start = right_index;
            while left_index < left.len() && left[left_index].is_ascii_digit() {
                left_index += 1;
            }
            while right_index < right.len() && right[right_index].is_ascii_digit() {
                right_index += 1;
            }
            let left_digits = trim_leading_ascii_zeroes(&left[left_start..left_index]);
            let right_digits = trim_leading_ascii_zeroes(&right[right_start..right_index]);
            let len_order = left_digits.len().cmp(&right_digits.len());
            if !len_order.is_eq() {
                return len_order;
            }
            let digit_order = left_digits.cmp(right_digits);
            if !digit_order.is_eq() {
                return digit_order;
            }
            let original_len_order = (right_index - right_start).cmp(&(left_index - left_start));
            if !original_len_order.is_eq() {
                return original_len_order;
            }
            continue;
        }
        let order = left_byte.cmp(&right_byte);
        if !order.is_eq() {
            return order;
        }
        left_index += 1;
        right_index += 1;
    }
    left.len().cmp(&right.len())
}

fn trim_leading_ascii_zeroes(bytes: &[u8]) -> &[u8] {
    let trimmed = bytes
        .iter()
        .position(|byte| *byte != b'0')
        .unwrap_or(bytes.len());
    &bytes[trimmed..]
}

pub(super) fn sort_entries_stable<F>(
    entries: &mut [(ArrayKey, Value)],
    mut compare_entries: F,
) -> Result<(), ArrayCallbackError>
where
    F: FnMut(
        &(ArrayKey, Value),
        &(ArrayKey, Value),
    ) -> Result<std::cmp::Ordering, ArrayCallbackError>,
{
    let mut sortable = entries
        .iter()
        .cloned()
        .enumerate()
        .map(|(original_index, entry)| SortableArrayEntry {
            original_index,
            entry,
        })
        .collect::<Vec<_>>();
    zend_sort_entries(&mut sortable, 0, entries.len(), &mut compare_entries)?;
    for (target, sorted) in entries.iter_mut().zip(sortable) {
        *target = sorted.entry;
    }
    Ok(())
}

#[derive(Clone)]
struct SortableArrayEntry {
    original_index: usize,
    entry: (ArrayKey, Value),
}

fn compare_sortable_entries<F>(
    entries: &[SortableArrayEntry],
    left: usize,
    right: usize,
    compare_entries: &mut F,
) -> Result<std::cmp::Ordering, ArrayCallbackError>
where
    F: FnMut(
        &(ArrayKey, Value),
        &(ArrayKey, Value),
    ) -> Result<std::cmp::Ordering, ArrayCallbackError>,
{
    let ordering = compare_entries(&entries[left].entry, &entries[right].entry)?;
    if ordering.is_eq() {
        Ok(entries[left]
            .original_index
            .cmp(&entries[right].original_index))
    } else {
        Ok(ordering)
    }
}

fn sortable_gt<F>(
    entries: &[SortableArrayEntry],
    left: usize,
    right: usize,
    compare_entries: &mut F,
) -> Result<bool, ArrayCallbackError>
where
    F: FnMut(
        &(ArrayKey, Value),
        &(ArrayKey, Value),
    ) -> Result<std::cmp::Ordering, ArrayCallbackError>,
{
    Ok(compare_sortable_entries(entries, left, right, compare_entries)?.is_gt())
}

fn zend_sort_2<F>(
    entries: &mut [SortableArrayEntry],
    a: usize,
    b: usize,
    compare_entries: &mut F,
) -> Result<(), ArrayCallbackError>
where
    F: FnMut(
        &(ArrayKey, Value),
        &(ArrayKey, Value),
    ) -> Result<std::cmp::Ordering, ArrayCallbackError>,
{
    if sortable_gt(entries, a, b, compare_entries)? {
        entries.swap(a, b);
    }
    Ok(())
}

fn zend_sort_3<F>(
    entries: &mut [SortableArrayEntry],
    a: usize,
    b: usize,
    c: usize,
    compare_entries: &mut F,
) -> Result<(), ArrayCallbackError>
where
    F: FnMut(
        &(ArrayKey, Value),
        &(ArrayKey, Value),
    ) -> Result<std::cmp::Ordering, ArrayCallbackError>,
{
    if !sortable_gt(entries, a, b, compare_entries)? {
        if !sortable_gt(entries, b, c, compare_entries)? {
            return Ok(());
        }
        entries.swap(b, c);
        if sortable_gt(entries, a, b, compare_entries)? {
            entries.swap(a, b);
        }
        return Ok(());
    }
    if !sortable_gt(entries, c, b, compare_entries)? {
        entries.swap(a, c);
        return Ok(());
    }
    entries.swap(a, b);
    if sortable_gt(entries, b, c, compare_entries)? {
        entries.swap(b, c);
    }
    Ok(())
}

fn zend_sort_4<F>(
    entries: &mut [SortableArrayEntry],
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    compare_entries: &mut F,
) -> Result<(), ArrayCallbackError>
where
    F: FnMut(
        &(ArrayKey, Value),
        &(ArrayKey, Value),
    ) -> Result<std::cmp::Ordering, ArrayCallbackError>,
{
    zend_sort_3(entries, a, b, c, compare_entries)?;
    if sortable_gt(entries, c, d, compare_entries)? {
        entries.swap(c, d);
        if sortable_gt(entries, b, c, compare_entries)? {
            entries.swap(b, c);
            if sortable_gt(entries, a, b, compare_entries)? {
                entries.swap(a, b);
            }
        }
    }
    Ok(())
}

fn zend_sort_5<F>(
    entries: &mut [SortableArrayEntry],
    a: usize,
    b: usize,
    c: usize,
    d: usize,
    e: usize,
    compare_entries: &mut F,
) -> Result<(), ArrayCallbackError>
where
    F: FnMut(
        &(ArrayKey, Value),
        &(ArrayKey, Value),
    ) -> Result<std::cmp::Ordering, ArrayCallbackError>,
{
    zend_sort_4(entries, a, b, c, d, compare_entries)?;
    if sortable_gt(entries, d, e, compare_entries)? {
        entries.swap(d, e);
        if sortable_gt(entries, c, d, compare_entries)? {
            entries.swap(c, d);
            if sortable_gt(entries, b, c, compare_entries)? {
                entries.swap(b, c);
                if sortable_gt(entries, a, b, compare_entries)? {
                    entries.swap(a, b);
                }
            }
        }
    }
    Ok(())
}

fn zend_insert_sort_entries<F>(
    entries: &mut [SortableArrayEntry],
    start: usize,
    count: usize,
    compare_entries: &mut F,
) -> Result<(), ArrayCallbackError>
where
    F: FnMut(
        &(ArrayKey, Value),
        &(ArrayKey, Value),
    ) -> Result<std::cmp::Ordering, ArrayCallbackError>,
{
    match count {
        0 | 1 => {}
        2 => zend_sort_2(entries, start, start + 1, compare_entries)?,
        3 => zend_sort_3(entries, start, start + 1, start + 2, compare_entries)?,
        4 => zend_sort_4(
            entries,
            start,
            start + 1,
            start + 2,
            start + 3,
            compare_entries,
        )?,
        5 => zend_sort_5(
            entries,
            start,
            start + 1,
            start + 2,
            start + 3,
            start + 4,
            compare_entries,
        )?,
        _ => {
            let end = start + count;
            let sentry = start + 6;
            for i in start + 1..sentry {
                let mut j = i - 1;
                if !sortable_gt(entries, j, i, compare_entries)? {
                    continue;
                }
                while j != start {
                    j -= 1;
                    if !sortable_gt(entries, j, i, compare_entries)? {
                        j += 1;
                        break;
                    }
                }
                for k in (j + 1..=i).rev() {
                    entries.swap(k, k - 1);
                }
            }
            for i in sentry..end {
                let mut j = i - 1;
                if !sortable_gt(entries, j, i, compare_entries)? {
                    continue;
                }
                loop {
                    j -= 2;
                    if !sortable_gt(entries, j, i, compare_entries)? {
                        j += 1;
                        if !sortable_gt(entries, j, i, compare_entries)? {
                            j += 1;
                        }
                        break;
                    }
                    if j == start {
                        break;
                    }
                    if j == start + 1 {
                        j -= 1;
                        if sortable_gt(entries, i, j, compare_entries)? {
                            j += 1;
                        }
                        break;
                    }
                }
                for k in (j + 1..=i).rev() {
                    entries.swap(k, k - 1);
                }
            }
        }
    }
    Ok(())
}

fn zend_sort_entries<F>(
    entries: &mut [SortableArrayEntry],
    mut start: usize,
    mut count: usize,
    compare_entries: &mut F,
) -> Result<(), ArrayCallbackError>
where
    F: FnMut(
        &(ArrayKey, Value),
        &(ArrayKey, Value),
    ) -> Result<std::cmp::Ordering, ArrayCallbackError>,
{
    loop {
        if count <= 16 {
            zend_insert_sort_entries(entries, start, count, compare_entries)?;
            return Ok(());
        }
        let end = start + count;
        let offset = count >> 1;
        let pivot = start + offset;
        if (count >> 10) != 0 {
            let delta = offset >> 1;
            zend_sort_5(
                entries,
                start,
                start + delta,
                pivot,
                pivot + delta,
                end - 1,
                compare_entries,
            )?;
        } else {
            zend_sort_3(entries, start, pivot, end - 1, compare_entries)?;
        }
        entries.swap(start + 1, pivot);
        let pivot = start + 1;
        let mut i = pivot + 1;
        let mut j = end - 1;
        loop {
            while sortable_gt(entries, pivot, i, compare_entries)? {
                i += 1;
                if i == j {
                    break;
                }
            }
            if i == j {
                break;
            }
            j -= 1;
            if j == i {
                break;
            }
            while sortable_gt(entries, j, pivot, compare_entries)? {
                j -= 1;
                if j == i {
                    break;
                }
            }
            if j == i {
                break;
            }
            entries.swap(i, j);
            i += 1;
            if i == j {
                break;
            }
        }
        entries.swap(pivot, i - 1);
        if (i - 1) - start < end - i {
            zend_sort_entries(entries, start, i - start - 1, compare_entries)?;
            start = i;
            count = end - i;
        } else {
            zend_sort_entries(entries, i, end - i, compare_entries)?;
            count = i - start - 1;
        }
    }
}
