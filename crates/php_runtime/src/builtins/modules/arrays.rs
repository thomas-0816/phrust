//! Arrays builtin registry slice.

use super::core::*;
use crate::builtins::{BuiltinCompatibility, BuiltinEntry};
use crate::builtins::{BuiltinContext, BuiltinError, BuiltinResult, RuntimeSourceSpan};
use crate::{ArrayKey, NumericValue, PhpArray, Value, to_bool, to_number};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "array_all",
        builtin_array_callback_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_any",
        builtin_array_callback_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_chunk",
        builtin_array_chunk,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_change_key_case",
        builtin_array_change_key_case,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_column",
        builtin_array_column,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("array_diff", builtin_array_diff, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "array_diff_assoc",
        builtin_array_diff_assoc,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_diff_key",
        builtin_array_diff_key,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_filter",
        builtin_array_callback_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_combine",
        builtin_array_combine,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("array_fill", builtin_array_fill, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "array_fill_keys",
        builtin_array_fill_keys,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_intersect",
        builtin_array_intersect,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_intersect_assoc",
        builtin_array_intersect_assoc,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_intersect_key",
        builtin_array_intersect_key,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_intersect_uassoc",
        builtin_array_intersect_uassoc,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_intersect_ukey",
        builtin_array_intersect_ukey,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_find",
        builtin_array_callback_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_find_key",
        builtin_array_callback_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("array_flip", builtin_array_flip, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "array_is_list",
        builtin_array_is_list,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_key_exists",
        builtin_array_key_exists,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_key_first",
        builtin_array_key_first,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_key_last",
        builtin_array_key_last,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("array_keys", builtin_array_keys, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "array_map",
        builtin_array_callback_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_merge",
        builtin_array_merge,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_merge_recursive",
        builtin_array_merge_recursive,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("array_pad", builtin_array_pad, BuiltinCompatibility::Php),
    BuiltinEntry::new("array_pop", builtin_array_pop, BuiltinCompatibility::Php),
    BuiltinEntry::new("array_push", builtin_array_push, BuiltinCompatibility::Php),
    BuiltinEntry::new("array_rand", builtin_array_rand, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "array_reduce",
        builtin_array_callback_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_replace",
        builtin_array_replace,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_replace_recursive",
        builtin_array_replace_recursive,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_reverse",
        builtin_array_reverse,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_search",
        builtin_array_search,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_shift",
        builtin_array_shift,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_slice",
        builtin_array_slice,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_splice",
        builtin_array_splice,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("array_sum", builtin_array_sum, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "array_unshift",
        builtin_array_unshift,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_uintersect",
        builtin_array_uintersect,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_uintersect_uassoc",
        builtin_array_uintersect_uassoc,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_unique",
        builtin_array_unique,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_values",
        builtin_array_values,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_walk",
        builtin_array_callback_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_walk_recursive",
        builtin_array_callback_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "array_multisort",
        builtin_array_sort_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "arsort",
        builtin_array_sort_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "asort",
        builtin_array_sort_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("count", builtin_count, BuiltinCompatibility::Php),
    BuiltinEntry::new("sizeof", builtin_count, BuiltinCompatibility::Php),
    BuiltinEntry::new("current", builtin_current, BuiltinCompatibility::Php),
    BuiltinEntry::new("end", builtin_end, BuiltinCompatibility::Php),
    BuiltinEntry::new("in_array", builtin_in_array, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "krsort",
        builtin_array_sort_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ksort",
        builtin_array_sort_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "key_exists",
        builtin_array_key_exists,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("key", builtin_key, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "natcasesort",
        builtin_array_sort_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "natsort",
        builtin_array_sort_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("next", builtin_next, BuiltinCompatibility::Php),
    BuiltinEntry::new("prev", builtin_prev, BuiltinCompatibility::Php),
    BuiltinEntry::new("range", builtin_range, BuiltinCompatibility::Php),
    BuiltinEntry::new("reset", builtin_reset, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "rsort",
        builtin_array_sort_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("shuffle", builtin_shuffle, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "sort",
        builtin_array_sort_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "uasort",
        builtin_array_sort_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "uksort",
        builtin_array_sort_requires_vm,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "usort",
        builtin_array_sort_requires_vm,
        BuiltinCompatibility::Php,
    ),
];

pub(in crate::builtins::modules) fn builtin_count(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("count", "one or two argument(s)"));
    }
    let mode = args
        .get(1)
        .map(|value| int_arg("count", value))
        .transpose()?
        .unwrap_or(0);
    let count = match deref_value(&args[0]) {
        Value::Array(array) if mode == 1 => count_recursive(&array),
        Value::Array(array) => array.len(),
        Value::Object(object) => {
            match (
                object.get_property("__entries"),
                object.get_property("__storage"),
            ) {
                (Some(Value::Array(entries)), _) => entries.len(),
                (_, Some(Value::Array(entries))) => entries.len(),
                _ => return Err(type_error("count", "array or Countable", &args[0])),
            }
        }
        _ => return Err(type_error("count", "array or Countable", &args[0])),
    };
    Ok(Value::Int(count as i64))
}

pub(in crate::builtins::modules) fn builtin_array_key_exists(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("array_key_exists", &args, 2)?;
    let key = array_key_arg("array_key_exists", &args[0])?;
    let Value::Array(array) = deref_value(&args[1]) else {
        return Err(type_error("array_key_exists", "array", &args[1]));
    };
    Ok(Value::Bool(array.get(&key).is_some()))
}

pub(in crate::builtins::modules) fn builtin_array_change_key_case(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error(
            "array_change_key_case",
            "one or two argument(s)",
        ));
    }
    let Value::Array(array) = deref_value(&args[0]) else {
        return Err(type_error("array_change_key_case", "array", &args[0]));
    };
    let case = args
        .get(1)
        .map(|value| int_arg("array_change_key_case", value))
        .transpose()?
        .unwrap_or(0);
    let mut output = PhpArray::new();
    for (key, value) in array.iter() {
        let key = match key {
            ArrayKey::Int(value) => ArrayKey::Int(*value),
            ArrayKey::String(value) if case == 1 => ArrayKey::String(crate::PhpString::from_bytes(
                value.to_string_lossy().to_uppercase().into_bytes(),
            )),
            ArrayKey::String(value) => ArrayKey::String(crate::PhpString::from_bytes(
                value.to_string_lossy().to_lowercase().into_bytes(),
            )),
        };
        output.insert(key, value.clone());
    }
    Ok(Value::Array(output))
}

pub(in crate::builtins::modules) fn builtin_array_keys(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=3).contains(&args.len()) {
        return Err(arity_error("array_keys", "one to three argument(s)"));
    }
    let Value::Array(array) = deref_value(&args[0]) else {
        return Err(type_error("array_keys", "array", &args[0]));
    };
    let strict = args
        .get(2)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("array_keys", message))?
        .unwrap_or(false);
    let mut keys = Vec::new();
    for (key, value) in array.iter() {
        if let Some(filter) = args.get(1)
            && !array_value_matches("array_keys", value, filter, strict)?
        {
            continue;
        }
        keys.push(array_key_to_value(key));
    }
    Ok(Value::packed_array(keys))
}

pub(in crate::builtins::modules) fn builtin_array_values(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("array_values", &args, 1)?;
    let Value::Array(array) = deref_value(&args[0]) else {
        return Err(type_error("array_values", "array", &args[0]));
    };
    Ok(Value::packed_array(
        array.iter().map(|(_, value)| value.clone()).collect(),
    ))
}

pub(in crate::builtins::modules) fn builtin_array_sum(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("array_sum", &args, 1)?;
    let Value::Array(array) = deref_value(&args[0]) else {
        return Err(type_error("array_sum", "array", &args[0]));
    };

    let mut int_total = 0i64;
    let mut float_total = 0.0f64;
    let mut use_float = false;
    for (_, value) in array.iter() {
        match to_number(value).map_err(|message| conversion_error("array_sum", message))? {
            NumericValue::Int(value) if !use_float => {
                if let Some(total) = int_total.checked_add(value) {
                    int_total = total;
                } else {
                    use_float = true;
                    float_total = int_total as f64 + value as f64;
                }
            }
            NumericValue::Int(value) => {
                float_total += value as f64;
            }
            NumericValue::Float(value) if !use_float => {
                use_float = true;
                float_total = int_total as f64 + value;
            }
            NumericValue::Float(value) => {
                float_total += value;
            }
        }
    }

    if use_float {
        Ok(Value::float(float_total))
    } else {
        Ok(Value::Int(int_total))
    }
}

pub(in crate::builtins::modules) fn builtin_array_combine(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("array_combine", &args, 2)?;
    let Value::Array(keys) = deref_value(&args[0]) else {
        return Err(type_error("array_combine", "array", &args[0]));
    };
    let Value::Array(values) = deref_value(&args[1]) else {
        return Err(type_error("array_combine", "array", &args[1]));
    };
    if keys.len() != values.len() {
        return Err(value_error(
            "array_combine",
            "Argument #1 ($keys) and argument #2 ($values) must have the same number of elements",
        ));
    }
    let mut output = PhpArray::new();
    for ((_, key), (_, value)) in keys.iter().zip(values.iter()) {
        let Some(key) = ArrayKey::from_value(key) else {
            return Err(type_error("array_combine", "array key", key));
        };
        output.insert(key, value.clone());
    }
    Ok(Value::Array(output))
}

pub(in crate::builtins::modules) fn builtin_array_is_list(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("array_is_list", &args, 1)?;
    let Value::Array(array) = deref_value(&args[0]) else {
        return Err(type_error("array_is_list", "array", &args[0]));
    };
    Ok(Value::Bool(array.packed_elements().is_some()))
}

pub(in crate::builtins::modules) fn builtin_array_key_first(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("array_key_first", &args, 1)?;
    let Value::Array(array) = deref_value(&args[0]) else {
        return Err(type_error("array_key_first", "array", &args[0]));
    };
    Ok(array
        .iter()
        .next()
        .map_or(Value::Null, |(key, _)| array_key_to_value(key)))
}

pub(in crate::builtins::modules) fn builtin_array_key_last(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("array_key_last", &args, 1)?;
    let Value::Array(array) = deref_value(&args[0]) else {
        return Err(type_error("array_key_last", "array", &args[0]));
    };
    Ok(array
        .iter()
        .last()
        .map_or(Value::Null, |(key, _)| array_key_to_value(key)))
}

pub(in crate::builtins::modules) fn builtin_in_array(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("in_array", "two or three argument(s)"));
    }
    let Value::Array(array) = deref_value(&args[1]) else {
        return Err(type_error("in_array", "array", &args[1]));
    };
    let strict = args
        .get(2)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("in_array", message))?
        .unwrap_or(false);
    for (_, value) in array.iter() {
        if array_value_matches("in_array", &args[0], value, strict)? {
            return Ok(Value::Bool(true));
        }
    }
    Ok(Value::Bool(false))
}

pub(in crate::builtins::modules) fn builtin_array_search(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("array_search", "two or three argument(s)"));
    }
    let Value::Array(array) = deref_value(&args[1]) else {
        return Err(type_error("array_search", "array", &args[1]));
    };
    let strict = args
        .get(2)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("array_search", message))?
        .unwrap_or(false);
    for (key, value) in array.iter() {
        if array_value_matches("array_search", &args[0], value, strict)? {
            return Ok(array_key_to_value(key));
        }
    }
    Ok(Value::Bool(false))
}

pub(in crate::builtins::modules) fn builtin_range(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("range", "two or three argument(s)"));
    }
    range_null_deprecation(context, &args[0], "#1 ($start)", span.clone());
    range_null_deprecation(context, &args[1], "#2 ($end)", span.clone());
    let step = args
        .get(2)
        .map(range_step_arg)
        .transpose()?
        .unwrap_or(RangeStep::Int(1));
    validate_range_step(step)?;

    if let Some(values) = range_string_values(context, &args[0], &args[1], step, span.clone())? {
        return Ok(Value::packed_array(values));
    }
    warn_range_null_string_boundary(context, &args[0], &args[1], span.clone());

    let start = range_numeric_arg("range", "#1 ($start)", &args[0])?;
    let end = range_numeric_arg("range", "#2 ($end)", &args[1])?;
    range_numeric_values(start, end, step).map(Value::packed_array)
}

pub(in crate::builtins::modules) fn builtin_array_column(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("array_column", "two or three argument(s)"));
    }
    let Value::Array(rows) = deref_value(&args[0]) else {
        return Err(type_error("array_column", "array", &args[0]));
    };
    let column_key = if matches!(deref_value(&args[1]), Value::Null) {
        None
    } else {
        Some(array_key_arg("array_column", &args[1])?)
    };
    let index_key = args
        .get(2)
        .filter(|value| !matches!(deref_value(value), Value::Null))
        .map(|value| array_key_arg("array_column", value))
        .transpose()?;
    let mut output = crate::PhpArray::new();
    for (_, row) in rows.iter() {
        let Value::Array(row) = deref_value(row) else {
            continue;
        };
        let Some(value) = column_key
            .as_ref()
            .map_or(Some(Value::Array(row.clone())), |key| row.get(key).cloned())
        else {
            continue;
        };
        if let Some(index_key) = &index_key
            && let Some(index_value) = row.get(index_key)
            && let Some(output_key) = ArrayKey::from_value(index_value)
        {
            output.insert(output_key, value);
            continue;
        }
        output.append(value);
    }
    Ok(Value::Array(output))
}

pub(in crate::builtins::modules) fn builtin_array_diff(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 {
        return Err(arity_error("array_diff", "at least two argument(s)"));
    }
    let first = array_value_arg("array_diff", &args[0])?;
    let others = array_list_arg("array_diff", &args[1..])?;
    Ok(Value::Array(array_diff_by_value(&first, &others)?))
}

pub(in crate::builtins::modules) fn builtin_array_diff_assoc(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 {
        return Err(arity_error("array_diff_assoc", "at least two argument(s)"));
    }
    let first = array_value_arg("array_diff_assoc", &args[0])?;
    let others = array_list_arg("array_diff_assoc", &args[1..])?;
    Ok(Value::Array(array_diff_by_key_and_value(&first, &others)?))
}

pub(in crate::builtins::modules) fn builtin_array_diff_key(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 {
        return Err(arity_error("array_diff_key", "at least two argument(s)"));
    }
    let first = array_value_arg("array_diff_key", &args[0])?;
    let others = array_list_arg("array_diff_key", &args[1..])?;
    let mut output = PhpArray::new();
    for (key, value) in first.iter() {
        if others.iter().all(|array| array.get(key).is_none()) {
            output.insert(key.clone(), value.clone());
        }
    }
    Ok(Value::Array(output))
}

pub(in crate::builtins::modules) fn builtin_array_fill(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("array_fill", &args, 3)?;
    let start_index = int_arg("array_fill", &args[0])?;
    let count = int_arg("array_fill", &args[1])?;
    if count < 0 {
        return Err(argument_value_error(
            "array_fill",
            "#2 ($count)",
            "must be greater than or equal to 0",
        ));
    }
    let count = usize::try_from(count).map_err(|_| {
        argument_value_error(
            "array_fill",
            "#2 ($count)",
            "must be less than or equal to PHP_INT_MAX",
        )
    })?;
    ensure_array_fill_size(count)?;

    let mut output = crate::PhpArray::new();
    for offset in 0..count {
        let offset = i64::try_from(offset).map_err(|_| {
            value_error(
                "array_fill",
                "The supplied range exceeds the maximum array size",
            )
        })?;
        let key = start_index.checked_add(offset).ok_or_else(|| {
            value_error(
                "array_fill",
                "The supplied range exceeds the maximum array size",
            )
        })?;
        output.insert(ArrayKey::Int(key), args[2].clone());
    }
    Ok(Value::Array(output))
}

pub(in crate::builtins::modules) fn builtin_array_fill_keys(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("array_fill_keys", &args, 2)?;
    let Value::Array(keys) = deref_value(&args[0]) else {
        return Err(type_error("array_fill_keys", "array", &args[0]));
    };
    ensure_array_fill_size(keys.len())?;

    let mut output = PhpArray::new();
    for (_, key) in keys.iter() {
        let key = crate::convert::to_string(key)
            .map(ArrayKey::from_php_string)
            .map_err(|message| conversion_error("array_fill_keys", message))?;
        output.insert(key, args[1].clone());
    }
    Ok(Value::Array(output))
}

pub(in crate::builtins::modules) fn builtin_array_intersect(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 {
        return Err(arity_error("array_intersect", "at least two argument(s)"));
    }
    let first = array_value_arg("array_intersect", &args[0])?;
    let others = array_list_arg("array_intersect", &args[1..])?;
    Ok(Value::Array(array_intersect_by_value(&first, &others)?))
}

pub(in crate::builtins::modules) fn builtin_array_intersect_assoc(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 {
        return Err(arity_error(
            "array_intersect_assoc",
            "at least two argument(s)",
        ));
    }
    let first = array_value_arg("array_intersect_assoc", &args[0])?;
    let others = array_list_arg("array_intersect_assoc", &args[1..])?;
    Ok(Value::Array(array_intersect_by_key_and_value(
        &first, &others,
    )?))
}

pub(in crate::builtins::modules) fn builtin_array_intersect_key(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 {
        return Err(arity_error(
            "array_intersect_key",
            "at least two argument(s)",
        ));
    }
    let first = array_value_arg("array_intersect_key", &args[0])?;
    let others = array_list_arg("array_intersect_key", &args[1..])?;
    let mut output = PhpArray::new();
    for (key, value) in first.iter() {
        if others.iter().all(|array| array.get(key).is_some()) {
            output.insert(key.clone(), value.clone());
        }
    }
    Ok(Value::Array(output))
}

pub(in crate::builtins::modules) fn builtin_array_intersect_ukey(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    array_callback_intersect_empty_shortcut("array_intersect_ukey", args, 1)
}

pub(in crate::builtins::modules) fn builtin_array_uintersect(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    array_callback_intersect_empty_shortcut("array_uintersect", args, 1)
}

pub(in crate::builtins::modules) fn builtin_array_intersect_uassoc(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    array_callback_intersect_empty_shortcut("array_intersect_uassoc", args, 1)
}

pub(in crate::builtins::modules) fn builtin_array_uintersect_uassoc(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    array_callback_intersect_empty_shortcut("array_uintersect_uassoc", args, 2)
}

pub(in crate::builtins::modules) fn builtin_array_push(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() {
        return Err(arity_error("array_push", "one or more argument(s)"));
    }
    let cell = array_reference_cell("array_push", &args[0])?;
    let mut array = array_from_reference_cell("array_push", &cell)?;
    for value in args.iter().skip(1) {
        array.append(value.clone());
    }
    let len = array.len() as i64;
    cell.set(Value::Array(array));
    Ok(Value::Int(len))
}

pub(in crate::builtins::modules) fn builtin_array_rand(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("array_rand", "one or two argument(s)"));
    }
    let array = array_value_arg("array_rand", &args[0])?;
    if array.is_empty() {
        return Err(value_error("array_rand", "Array is empty"));
    }
    let requested = args
        .get(1)
        .map(|value| int_arg("array_rand", value))
        .transpose()?
        .unwrap_or(1);
    if requested < 1 || requested as usize > array.len() {
        return Err(value_error(
            "array_rand",
            "Argument #2 ($num) must be between 1 and the number of elements in argument #1 ($array)",
        ));
    }

    let mut keys = array.iter().map(|(key, _)| key.clone()).collect::<Vec<_>>();
    let requested = requested as usize;
    for index in 0..requested {
        let offset = random_bounded_usize("array_rand", keys.len() - index)?;
        keys.swap(index, index + offset);
    }

    if requested == 1 {
        Ok(array_key_to_value(&keys[0]))
    } else {
        Ok(Value::packed_array(
            keys.into_iter()
                .take(requested)
                .map(|key| array_key_to_value(&key))
                .collect(),
        ))
    }
}

pub(in crate::builtins::modules) fn builtin_shuffle(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("shuffle", &args, 1)?;
    let cell = array_reference_cell("shuffle", &args[0])?;
    let array = array_from_reference_cell("shuffle", &cell)?;
    let mut values = array
        .iter()
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    for index in 0..values.len() {
        let offset = random_bounded_usize("shuffle", values.len() - index)?;
        values.swap(index, index + offset);
    }
    cell.set(Value::packed_array(values));
    Ok(Value::Bool(true))
}

pub(in crate::builtins::modules) fn builtin_current(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("current", &args, 1)?;
    let array = array_value_arg("current", &args[0])?;
    Ok(array.pointer_value().unwrap_or(Value::Bool(false)))
}

pub(in crate::builtins::modules) fn builtin_key(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("key", &args, 1)?;
    let array = array_value_arg("key", &args[0])?;
    Ok(array
        .pointer_key()
        .map_or(Value::Null, |key| array_key_to_value(&key)))
}

pub(in crate::builtins::modules) fn builtin_next(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("next", &args, 1)?;
    let cell = array_reference_cell("next", &args[0])?;
    let mut array = array_from_reference_cell("next", &cell)?;
    let value = array.next_pointer().unwrap_or(Value::Bool(false));
    cell.set(Value::Array(array));
    Ok(value)
}

pub(in crate::builtins::modules) fn builtin_prev(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("prev", &args, 1)?;
    let cell = array_reference_cell("prev", &args[0])?;
    let mut array = array_from_reference_cell("prev", &cell)?;
    let value = array.prev_pointer().unwrap_or(Value::Bool(false));
    cell.set(Value::Array(array));
    Ok(value)
}

pub(in crate::builtins::modules) fn builtin_end(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("end", &args, 1)?;
    let cell = array_reference_cell("end", &args[0])?;
    let mut array = array_from_reference_cell("end", &cell)?;
    let value = array.end_pointer().unwrap_or(Value::Bool(false));
    cell.set(Value::Array(array));
    Ok(value)
}

pub(in crate::builtins::modules) fn builtin_reset(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("reset", &args, 1)?;
    let cell = array_reference_cell("reset", &args[0])?;
    let mut array = array_from_reference_cell("reset", &cell)?;
    let value = array.reset_pointer().unwrap_or(Value::Bool(false));
    cell.set(Value::Array(array));
    Ok(value)
}

pub(in crate::builtins::modules) fn builtin_array_pop(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("array_pop", &args, 1)?;
    let cell = array_reference_cell("array_pop", &args[0])?;
    let mut array = array_from_reference_cell("array_pop", &cell)?;
    let value = array.pop().unwrap_or(Value::Null);
    cell.set(Value::Array(array));
    Ok(value)
}

pub(in crate::builtins::modules) fn builtin_array_shift(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("array_shift", &args, 1)?;
    let cell = array_reference_cell("array_shift", &args[0])?;
    let array = array_from_reference_cell("array_shift", &cell)?;
    let mut entries = array_entries(&array);
    let value = if entries.is_empty() {
        Value::Null
    } else {
        entries.remove(0).1
    };
    cell.set(Value::Array(array_from_entries_reindex_ints(entries)));
    Ok(value)
}

pub(in crate::builtins::modules) fn builtin_array_unshift(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() {
        return Err(arity_error("array_unshift", "one or more argument(s)"));
    }
    let cell = array_reference_cell("array_unshift", &args[0])?;
    let array = array_from_reference_cell("array_unshift", &cell)?;
    let mut output = crate::PhpArray::new();
    for value in args.iter().skip(1) {
        output.append(value.clone());
    }
    for (key, value) in array.iter() {
        match key {
            ArrayKey::Int(_) => {
                output.append(value.clone());
            }
            ArrayKey::String(key) => {
                output.insert(ArrayKey::String(key.clone()), value.clone());
            }
        }
    }
    let len = output.len() as i64;
    cell.set(Value::Array(output));
    Ok(Value::Int(len))
}

pub(in crate::builtins::modules) fn builtin_array_slice(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error("array_slice", "two to four argument(s)"));
    }
    let array = array_value_arg("array_slice", &args[0])?;
    let offset = int_arg("array_slice", &args[1])?;
    let length = args
        .get(2)
        .filter(|value| !matches!(deref_value(value), Value::Null))
        .map(|value| int_arg("array_slice", value))
        .transpose()?;
    let preserve_keys = args
        .get(3)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("array_slice", message))?
        .unwrap_or(false);
    let entries = slice_entries(array_entries(&array), offset, length);
    Ok(Value::Array(array_from_entries_for_slice(
        entries,
        preserve_keys,
    )))
}

pub(in crate::builtins::modules) fn builtin_array_splice(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=4).contains(&args.len()) {
        return Err(arity_error("array_splice", "two to four argument(s)"));
    }
    let cell = array_reference_cell("array_splice", &args[0])?;
    let array = array_from_reference_cell("array_splice", &cell)?;
    let entries = array_entries(&array);
    let offset = int_arg("array_splice", &args[1])?;
    let start = normalize_slice_start(entries.len(), offset);
    let delete_len = args
        .get(2)
        .filter(|value| !matches!(deref_value(value), Value::Null))
        .map(|value| splice_length(entries.len(), start, int_arg("array_splice", value)?))
        .transpose()?
        .unwrap_or(entries.len().saturating_sub(start));
    let replacement = args
        .get(3)
        .map(|value| splice_replacement_values("array_splice", value))
        .transpose()?
        .unwrap_or_default();

    let removed = entries[start..start + delete_len].to_vec();
    let mut result_values = Vec::new();
    result_values.extend(entries[..start].iter().map(|(_, value)| value.clone()));
    result_values.extend(replacement);
    result_values.extend(
        entries[start + delete_len..]
            .iter()
            .map(|(_, value)| value.clone()),
    );
    cell.set(Value::packed_array(result_values));
    Ok(Value::Array(array_from_entries_reindex_ints(removed)))
}

pub(in crate::builtins::modules) fn builtin_array_unique(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("array_unique", "one or two argument(s)"));
    }
    let array = array_value_arg("array_unique", &args[0])?;
    let flags = args
        .get(1)
        .map(|value| int_arg("array_unique", value))
        .transpose()?
        .unwrap_or(SORT_STRING);
    let mut unique = Vec::new();
    let mut output = crate::PhpArray::new();

    for (key, value) in array.iter() {
        let candidate = array_unique_key(value, flags)?;
        if unique
            .iter()
            .any(|seen| array_unique_keys_match(seen, &candidate))
        {
            continue;
        }
        unique.push(candidate);
        output.insert(key.clone(), value.clone());
    }

    Ok(Value::Array(output))
}

pub(in crate::builtins::modules) fn builtin_array_merge(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let mut output = crate::PhpArray::new();
    for arg in &args {
        let array = array_value_arg("array_merge", arg)?;
        for (key, value) in array.iter() {
            match key {
                ArrayKey::Int(_) => {
                    output.append(value.clone());
                }
                ArrayKey::String(key) => {
                    output.insert(ArrayKey::String(key.clone()), value.clone());
                }
            }
        }
    }
    Ok(Value::Array(output))
}

pub(in crate::builtins::modules) fn builtin_array_merge_recursive(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    let mut output = crate::PhpArray::new();
    for arg in &args {
        let array = array_value_arg("array_merge_recursive", arg)?;
        merge_recursive_into(&mut output, &array);
    }
    Ok(Value::Array(output))
}

pub(in crate::builtins::modules) fn builtin_array_replace(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() {
        return Err(arity_error("array_replace", "one or more argument(s)"));
    }
    let mut output = array_value_arg("array_replace", &args[0])?;
    for arg in args.iter().skip(1) {
        let array = array_value_arg("array_replace", arg)?;
        for (key, value) in array.iter() {
            output.insert(key.clone(), value.clone());
        }
    }
    Ok(Value::Array(output))
}

pub(in crate::builtins::modules) fn builtin_array_replace_recursive(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() {
        return Err(arity_error(
            "array_replace_recursive",
            "one or more argument(s)",
        ));
    }
    let mut output = array_value_arg("array_replace_recursive", &args[0])?;
    for arg in args.iter().skip(1) {
        let array = array_value_arg("array_replace_recursive", arg)?;
        replace_recursive_into(&mut output, &array);
    }
    Ok(Value::Array(output))
}

pub(in crate::builtins::modules) fn builtin_array_reverse(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("array_reverse", "one or two argument(s)"));
    }
    let array = array_value_arg("array_reverse", &args[0])?;
    let preserve_keys = args
        .get(1)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("array_reverse", message))?
        .unwrap_or(false);
    let mut entries = array_entries(&array);
    entries.reverse();
    Ok(Value::Array(array_from_entries_for_slice(
        entries,
        preserve_keys,
    )))
}

pub(in crate::builtins::modules) fn builtin_array_pad(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("array_pad", &args, 3)?;
    let array = array_value_arg("array_pad", &args[0])?;
    let target = int_arg("array_pad", &args[1])?;
    let pad_value = args[2].clone();
    let mut values = array
        .iter()
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    let target_len = target.unsigned_abs() as usize;
    if target_len > values.len() {
        let pad_count = target_len - values.len();
        if target < 0 {
            let mut padded = vec![pad_value; pad_count];
            padded.extend(values);
            values = padded;
        } else {
            values.extend(std::iter::repeat_n(pad_value, pad_count));
        }
    }
    Ok(Value::packed_array(values))
}

pub(in crate::builtins::modules) fn builtin_array_chunk(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(2..=3).contains(&args.len()) {
        return Err(arity_error("array_chunk", "two or three argument(s)"));
    }
    let array = array_value_arg("array_chunk", &args[0])?;
    let length = int_arg("array_chunk", &args[1])?;
    if length <= 0 {
        return Err(value_error(
            "array_chunk",
            "length must be greater than or equal to 1",
        ));
    }
    let preserve_keys = args
        .get(2)
        .map(to_bool)
        .transpose()
        .map_err(|message| conversion_error("array_chunk", message))?
        .unwrap_or(false);
    let entries = array_entries(&array);
    let mut chunks = Vec::new();
    for chunk in entries.chunks(length as usize) {
        let chunk_entries = chunk.to_vec();
        let chunk_array = if preserve_keys {
            array_from_entries_preserve(chunk_entries)
        } else {
            PhpArray::from_packed(chunk_entries.into_iter().map(|(_, value)| value).collect())
        };
        chunks.push(Value::Array(chunk_array));
    }
    Ok(Value::packed_array(chunks))
}

pub(in crate::builtins::modules) fn builtin_array_flip(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("array_flip", &args, 1)?;
    let array = array_value_arg("array_flip", &args[0])?;
    let mut output = crate::PhpArray::new();
    for (key, value) in array.iter() {
        let Some(output_key) = array_flip_key(value) else {
            context.php_warning(
                "E_PHP_RUNTIME_ARRAY_FLIP_ENTRY_SKIPPED",
                "array_flip(): Can only flip string and integer values, entry skipped",
                span.clone(),
            );
            continue;
        };
        output.insert(output_key, array_key_to_value(key));
    }
    Ok(Value::Array(output))
}

fn array_flip_key(value: &Value) -> Option<ArrayKey> {
    match deref_value(value) {
        Value::Int(value) => Some(ArrayKey::Int(value)),
        Value::String(value) => Some(ArrayKey::from_php_string(value.clone())),
        _ => None,
    }
}

pub(in crate::builtins::modules) fn builtin_array_callback_requires_vm(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_CALLABLE_CONTEXT_REQUIRED",
        "array callback builtins require VM callable dispatch",
    ))
}

pub(in crate::builtins::modules) fn builtin_array_sort_requires_vm(
    _context: &mut BuiltinContext<'_>,
    _args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_CALLABLE_CONTEXT_REQUIRED",
        "array sort builtins require VM reference and callable dispatch",
    ))
}
