//! Pcre builtin registry slice.

use super::core::*;
use crate::builtins::context::{PcreBuiltinServices, PcreCallbackServices, PcreServiceAccess};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinRegistry,
    BuiltinResult, RuntimeSourceSpan,
};
use crate::{CallableValue, PhpArray, PhpString, Value, pcre};
use pcre2::bytes::MatchOptions;
use std::sync::Arc;

type PregReplaceSpec = (Arc<pcre::CompiledPattern>, Vec<u8>);

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("preg_filter", exact_preg_filter, BuiltinCompatibility::Php),
    BuiltinEntry::new("preg_grep", exact_preg_grep, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "preg_last_error",
        exact_preg_last_error,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "preg_last_error_msg",
        exact_preg_last_error_msg,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("preg_match", exact_preg_match, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "preg_match_all",
        exact_preg_match_all,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("preg_quote", exact_preg_quote, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "preg_replace",
        exact_preg_replace,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "preg_replace_callback",
        builtin_preg_replace_callback,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "preg_replace_callback_array",
        builtin_preg_replace_callback_array,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("preg_split", exact_preg_split, BuiltinCompatibility::Php),
];

macro_rules! exact_pcre_builtin {
    ($entry:ident => $implementation:ident) => {
        #[doc(hidden)]
        pub fn $entry(
            context: &mut BuiltinContext<'_>,
            args: Vec<Value>,
            span: RuntimeSourceSpan,
        ) -> BuiltinResult {
            let mut services = context.pcre_services();
            $implementation(&mut services, args, span)
        }
    };
}

exact_pcre_builtin!(exact_preg_match => preg_match);
exact_pcre_builtin!(exact_preg_match_all => preg_match_all);
exact_pcre_builtin!(exact_preg_replace => preg_replace);
exact_pcre_builtin!(exact_preg_filter => preg_filter);
exact_pcre_builtin!(exact_preg_split => preg_split);
exact_pcre_builtin!(exact_preg_grep => preg_grep);
exact_pcre_builtin!(exact_preg_last_error => preg_last_error);
exact_pcre_builtin!(exact_preg_last_error_msg => preg_last_error_msg);

#[derive(Debug)]
pub struct NativePregMatchResult {
    pub matched: bool,
    pub captures: super::json::NativeJsonDecodedValue,
}

#[derive(Debug)]
pub struct NativePregMatchAllResult {
    pub count: i64,
    pub captures: super::json::NativeJsonDecodedValue,
}

#[derive(Debug)]
pub struct NativePregReplaceResult {
    pub bytes: Option<Vec<u8>>,
    pub count: i64,
}

#[derive(Debug)]
pub struct NativePregReplaceManyResult {
    pub values: Vec<Option<Vec<u8>>>,
    pub count: i64,
}

/// Runs `preg_match` over native bytes and returns a typed capture tree.
/// Mixed named/MARK capture maps remain on the exact baseline continuation.
#[doc(hidden)]
pub fn native_preg_match(
    state: &mut crate::builtins::PcreRequestState,
    limits: pcre::PcreMatchLimits,
    pattern: &[u8],
    subject: &[u8],
    flags: i64,
    offset: i64,
) -> Result<Option<NativePregMatchResult>, BuiltinError> {
    validate_preg_offset_min("preg_match", offset)?;
    validate_preg_match_flags("preg_match", "#4 ($flags)", flags)?;
    let Some(start) = preg_match_offset(subject.len(), offset) else {
        return Ok(None);
    };
    let compiled = match state.cache_mut().compile_bytes_with_limits(pattern, limits) {
        Ok(compiled) => compiled,
        Err(_) => return Ok(None),
    };
    if compiled.capture_names().iter().any(Option::is_some) {
        return Ok(None);
    }
    let options = match state
        .cache_mut()
        .match_options_for_subject_bytes_at_offset(&compiled, subject, start)
    {
        Ok(options) => options,
        Err(_) => return Ok(None),
    };
    let captures = match compiled.captures_at_with_options(subject, start, options) {
        Ok(captures) => captures,
        Err(_) => return Ok(None),
    };
    let Some(captures) = captures else {
        state.last_error_mut().clear();
        return Ok(Some(NativePregMatchResult {
            matched: false,
            captures: super::json::NativeJsonDecodedValue::Array(Vec::new()),
        }));
    };
    if captures.mark().is_some() {
        return Ok(None);
    }
    let count = if flags & pcre::PREG_UNMATCHED_AS_NULL != 0 {
        captures.len()
    } else {
        (0..captures.len())
            .rev()
            .find(|index| captures.get(*index).is_some())
            .map_or(0, |index| index + 1)
    };
    let mut output = Vec::with_capacity(count);
    for index in 0..count {
        let value = match captures.get(index) {
            Some(capture) if flags & pcre::PREG_OFFSET_CAPTURE != 0 => {
                super::json::NativeJsonDecodedValue::Array(vec![
                    super::json::NativeJsonDecodedValue::String(capture.as_bytes().to_vec()),
                    super::json::NativeJsonDecodedValue::Int(capture.start() as i64),
                ])
            }
            Some(capture) => {
                super::json::NativeJsonDecodedValue::String(capture.as_bytes().to_vec())
            }
            None if flags & pcre::PREG_OFFSET_CAPTURE != 0 => {
                super::json::NativeJsonDecodedValue::Array(vec![
                    if flags & pcre::PREG_UNMATCHED_AS_NULL != 0 {
                        super::json::NativeJsonDecodedValue::Null
                    } else {
                        super::json::NativeJsonDecodedValue::String(Vec::new())
                    },
                    super::json::NativeJsonDecodedValue::Int(-1),
                ])
            }
            None if flags & pcre::PREG_UNMATCHED_AS_NULL != 0 => {
                super::json::NativeJsonDecodedValue::Null
            }
            None => super::json::NativeJsonDecodedValue::String(Vec::new()),
        };
        output.push(value);
    }
    state.last_error_mut().clear();
    Ok(Some(NativePregMatchResult {
        matched: true,
        captures: super::json::NativeJsonDecodedValue::Array(output),
    }))
}

#[doc(hidden)]
pub fn native_preg_match_all(
    state: &mut crate::builtins::PcreRequestState,
    limits: pcre::PcreMatchLimits,
    pattern: &[u8],
    subject: &[u8],
    flags: i64,
    offset: i64,
) -> Result<Option<NativePregMatchAllResult>, BuiltinError> {
    validate_preg_match_all_flags(flags)?;
    validate_preg_offset_min("preg_match_all", offset)?;
    let Some(start) = preg_match_offset(subject.len(), offset) else {
        return Ok(None);
    };
    let compiled = match state.cache_mut().compile_bytes_with_limits(pattern, limits) {
        Ok(compiled) => compiled,
        Err(_) => return Ok(None),
    };
    if compiled.capture_names().iter().any(Option::is_some) {
        return Ok(None);
    }
    let options = match state
        .cache_mut()
        .match_options_for_subject_bytes_at_offset(&compiled, subject, start)
    {
        Ok(options) => options,
        Err(_) => return Ok(None),
    };
    let set_order = flags & pcre::PREG_SET_ORDER != 0;
    let mut matches = Vec::new();
    let mut unsupported = false;
    if compiled
        .for_each_php_match_with_options(
            subject,
            start,
            options,
            |captures| {
                if captures.mark().is_some() {
                    unsupported = true;
                    return Ok(false);
                }
                let count = if flags & pcre::PREG_UNMATCHED_AS_NULL != 0 || !set_order {
                    captures.len()
                } else {
                    (0..captures.len())
                        .rev()
                        .find(|index| captures.get(*index).is_some())
                        .map_or(0, |index| index + 1)
                };
                let mut row = Vec::with_capacity(count);
                for index in 0..count {
                    let value = match captures.get(index) {
                        Some(capture) if flags & pcre::PREG_OFFSET_CAPTURE != 0 => {
                            super::json::NativeJsonDecodedValue::Array(vec![
                                super::json::NativeJsonDecodedValue::String(
                                    capture.as_bytes().to_vec(),
                                ),
                                super::json::NativeJsonDecodedValue::Int(capture.start() as i64),
                            ])
                        }
                        Some(capture) => {
                            super::json::NativeJsonDecodedValue::String(capture.as_bytes().to_vec())
                        }
                        None if flags & pcre::PREG_OFFSET_CAPTURE != 0 => {
                            super::json::NativeJsonDecodedValue::Array(vec![
                                if flags & pcre::PREG_UNMATCHED_AS_NULL != 0 {
                                    super::json::NativeJsonDecodedValue::Null
                                } else {
                                    super::json::NativeJsonDecodedValue::String(Vec::new())
                                },
                                super::json::NativeJsonDecodedValue::Int(-1),
                            ])
                        }
                        None if flags & pcre::PREG_UNMATCHED_AS_NULL != 0 => {
                            super::json::NativeJsonDecodedValue::Null
                        }
                        None => super::json::NativeJsonDecodedValue::String(Vec::new()),
                    };
                    row.push(value);
                }
                matches.push(row);
                Ok(true)
            },
            std::convert::identity,
        )
        .is_err()
    {
        return Ok(None);
    }
    if unsupported {
        return Ok(None);
    }
    let count = matches.len() as i64;
    let captures = if set_order {
        super::json::NativeJsonDecodedValue::Array(
            matches
                .into_iter()
                .map(super::json::NativeJsonDecodedValue::Array)
                .collect(),
        )
    } else {
        let groups = compiled.capture_names().len();
        let mut columns = (0..groups).map(|_| Vec::new()).collect::<Vec<_>>();
        for row in matches {
            for (index, value) in row.into_iter().enumerate() {
                columns[index].push(value);
            }
        }
        super::json::NativeJsonDecodedValue::Array(
            columns
                .into_iter()
                .map(super::json::NativeJsonDecodedValue::Array)
                .collect(),
        )
    };
    state.last_error_mut().clear();
    Ok(Some(NativePregMatchAllResult { count, captures }))
}

/// Executes the scalar form shared by `preg_replace` and `preg_filter`
/// directly over native bytes, including capture expansion.
#[doc(hidden)]
pub fn native_preg_replace_scalar(
    state: &mut crate::builtins::PcreRequestState,
    limits: pcre::PcreMatchLimits,
    pattern: &[u8],
    replacement: &[u8],
    subject: &[u8],
    limit: i64,
    filter: bool,
) -> Option<NativePregReplaceResult> {
    let compiled = state
        .cache_mut()
        .compile_bytes_with_limits(pattern, limits)
        .ok()?;
    let mut count = 0;
    let bytes = preg_replace_bytes(&compiled, replacement, subject, limit, &mut count).ok()?;
    state.last_error_mut().clear();
    Some(NativePregReplaceResult {
        bytes: (!filter || count != 0).then_some(bytes),
        count,
    })
}

/// Executes one prepared scalar pattern/replacement over a direct array's
/// string subjects. Keys remain authoritative in the caller; this returns
/// only replacement bytes and the aggregate replacement count.
#[doc(hidden)]
pub fn native_preg_replace_many(
    state: &mut crate::builtins::PcreRequestState,
    limits: pcre::PcreMatchLimits,
    pattern: &[u8],
    replacement: &[u8],
    subjects: &[&[u8]],
    limit: i64,
    filter: bool,
) -> Option<NativePregReplaceManyResult> {
    let compiled = state
        .cache_mut()
        .compile_bytes_with_limits(pattern, limits)
        .ok()?;
    let mut count = 0;
    let mut values = Vec::with_capacity(subjects.len());
    for subject in subjects {
        let before = count;
        let replaced =
            preg_replace_bytes(&compiled, replacement, subject, limit, &mut count).ok()?;
        values.push((!filter || count != before).then_some(replaced));
    }
    state.last_error_mut().clear();
    Some(NativePregReplaceManyResult { values, count })
}

#[doc(hidden)]
pub fn native_preg_split(
    state: &mut crate::builtins::PcreRequestState,
    limits: pcre::PcreMatchLimits,
    pattern: &[u8],
    subject: &[u8],
    limit: i64,
    flags: i64,
) -> Option<super::json::NativeJsonDecodedValue> {
    let compiled = state
        .cache_mut()
        .compile_bytes_with_limits(pattern, limits)
        .ok()?;
    let options = state
        .cache_mut()
        .match_options_for_subject_bytes_at_offset(&compiled, subject, 0)
        .ok()?;
    let mut pieces = Vec::new();
    let mut last_end = 0usize;
    let mut emitted = 0i64;
    let append =
        |pieces: &mut Vec<super::json::NativeJsonDecodedValue>, bytes: &[u8], offset: usize| {
            if flags & pcre::PREG_SPLIT_NO_EMPTY != 0 && bytes.is_empty() {
                return;
            }
            let value = super::json::NativeJsonDecodedValue::String(bytes.to_vec());
            pieces.push(if flags & pcre::PREG_SPLIT_OFFSET_CAPTURE != 0 {
                super::json::NativeJsonDecodedValue::Array(vec![
                    value,
                    super::json::NativeJsonDecodedValue::Int(offset as i64),
                ])
            } else {
                value
            });
        };
    let walked = compiled.for_each_php_match_with_options(
        subject,
        0,
        options,
        |captures| {
            let Some(full) = captures.get(0) else {
                return Ok(true);
            };
            if limit > 0 && emitted >= limit - 1 {
                return Ok(false);
            }
            if full.start() < last_end {
                return Err(pcre::PcreFailure::new(
                    pcre::PREG_INTERNAL_ERROR,
                    "PCRE split match moved before the previous delimiter",
                ));
            }
            append(&mut pieces, &subject[last_end..full.start()], last_end);
            emitted += 1;
            if flags & pcre::PREG_SPLIT_DELIM_CAPTURE != 0 {
                for index in 1..captures.len() {
                    if let Some(capture) = captures.get(index) {
                        append(&mut pieces, capture.as_bytes(), capture.start());
                    }
                }
            }
            last_end = full.end();
            Ok(true)
        },
        std::convert::identity,
    );
    if walked.is_err() {
        return None;
    }
    append(&mut pieces, &subject[last_end..], last_end);
    state.last_error_mut().clear();
    Some(super::json::NativeJsonDecodedValue::Array(pieces))
}

/// Selects the input strings matched by `preg_grep` without constructing a
/// PHP array or PHP string representation. The caller keeps the authoritative
/// keys and values and uses this mask to publish the result array directly.
#[doc(hidden)]
pub fn native_preg_grep(
    state: &mut crate::builtins::PcreRequestState,
    limits: pcre::PcreMatchLimits,
    pattern: &[u8],
    subjects: &[&[u8]],
    flags: i64,
) -> Option<Vec<bool>> {
    let compiled = state
        .cache_mut()
        .compile_bytes_with_limits(pattern, limits)
        .ok()?;
    let invert = flags & pcre::PREG_GREP_INVERT != 0;
    let mut selected = Vec::with_capacity(subjects.len());
    for subject in subjects {
        let options = state
            .cache_mut()
            .match_options_for_subject_bytes_at_offset(&compiled, subject, 0)
            .ok()?;
        let is_match = compiled
            .captures_at_with_options(subject, 0, options)
            .ok()?
            .is_some();
        selected.push(is_match != invert);
    }
    state.last_error_mut().clear();
    Some(selected)
}

pub(in crate::builtins::modules) fn builtin_preg_replace_callback(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    let mut services = context.pcre_callback_services();
    preg_replace_callback(&mut services, args, span)
}

fn preg_match(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 5 {
        return Err(arity_error("preg_match", "two to five argument(s)"));
    }
    let pattern = string_needle_arg("preg_match", "#1 ($pattern)", &args[0])?;
    let subject = string_arg("preg_match", &args[1])?;
    let flags = args
        .get(3)
        .map(|value| int_arg("preg_match", value))
        .transpose()?
        .unwrap_or(0);
    let offset = args
        .get(4)
        .map(|value| int_arg("preg_match", value))
        .transpose()?
        .unwrap_or(0);
    validate_preg_offset_min("preg_match", offset)?;
    let subject_bytes = subject.as_bytes();
    let Some(start_offset) = preg_match_offset(subject_bytes.len(), offset) else {
        context.set_preg_last_error(
            pcre::PREG_INTERNAL_ERROR,
            pcre::preg_error_message(pcre::PREG_INTERNAL_ERROR),
        );
        assign_reference_arg(args.get(2), Value::packed_array(Vec::new()));
        return Ok(Value::Bool(false));
    };
    if let Ok(Some(literal)) = pcre::simple_literal_pattern(&pattern) {
        validate_preg_match_flags("preg_match", "#4 ($flags)", flags)?;
        return match find_literal_match(subject_bytes, literal.as_bytes(), start_offset) {
            Some((start, end)) => {
                assign_reference_arg(
                    args.get(2),
                    preg_single_capture_array(subject_bytes, start, end, flags),
                );
                context.clear_preg_last_error();
                Ok(Value::Int(1))
            }
            None => {
                assign_reference_arg(args.get(2), Value::packed_array(Vec::new()));
                context.clear_preg_last_error();
                Ok(Value::Int(0))
            }
        };
    }
    let Some(compiled) = compile_preg_pattern(context, "preg_match", pattern, span) else {
        return Ok(Value::Bool(false));
    };
    validate_preg_match_flags("preg_match", "#4 ($flags)", flags)?;
    let match_options = match context.pcre_cache().match_options_for_subject_at_offset(
        &compiled,
        &subject,
        start_offset,
    ) {
        Ok(options) => options,
        Err(error) => {
            assign_reference_arg(args.get(2), Value::packed_array(Vec::new()));
            return preg_failure(context, error);
        }
    };
    if let Some(fast_match) = compiled.fast_match_at(subject_bytes, start_offset) {
        return match fast_match {
            Some((start, end)) => {
                assign_reference_arg(
                    args.get(2),
                    preg_single_capture_array(subject_bytes, start, end, flags),
                );
                context.clear_preg_last_error();
                Ok(Value::Int(1))
            }
            None => {
                assign_reference_arg(args.get(2), Value::packed_array(Vec::new()));
                context.clear_preg_last_error();
                Ok(Value::Int(0))
            }
        };
    }
    match compiled.captures_at_with_options(subject_bytes, start_offset, match_options) {
        Ok(Some(captures)) => {
            let matches =
                pcre::captures_to_array_with_names(&captures, compiled.capture_names(), flags, 0);
            assign_reference_arg(args.get(2), matches);
            context.clear_preg_last_error();
            Ok(Value::Int(1))
        }
        Ok(None) => {
            assign_reference_arg(args.get(2), Value::packed_array(Vec::new()));
            context.clear_preg_last_error();
            Ok(Value::Int(0))
        }
        Err(error) => preg_failure(context, error),
    }
}

fn preg_single_capture_array(subject: &[u8], start: usize, end: usize, flags: i64) -> Value {
    let matched = Value::String(PhpString::intern(&subject[start..end]));
    let capture = if flags & pcre::PREG_OFFSET_CAPTURE != 0 {
        Value::packed_array(vec![matched, Value::Int(start as i64)])
    } else {
        matched
    };
    Value::packed_array(vec![capture])
}

fn find_literal_match(haystack: &[u8], needle: &[u8], start: usize) -> Option<(usize, usize)> {
    if needle.is_empty() || start > haystack.len() {
        return None;
    }
    let last_start = haystack.len().checked_sub(needle.len())?;
    let first = needle[0];
    let mut index = start;
    while index <= last_start {
        if haystack[index] == first && &haystack[index..index + needle.len()] == needle {
            return Some((index, index + needle.len()));
        }
        index += 1;
    }
    None
}

fn preg_match_all(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 5 {
        return Err(arity_error("preg_match_all", "two to five argument(s)"));
    }
    let pattern = string_needle_arg("preg_match_all", "#1 ($pattern)", &args[0])?;
    let subject = string_arg("preg_match_all", &args[1])?;
    let flags = args
        .get(3)
        .map(|value| int_arg("preg_match_all", value))
        .transpose()?
        .unwrap_or(pcre::PREG_PATTERN_ORDER);
    validate_preg_match_all_flags(flags)?;
    let offset = args
        .get(4)
        .map(|value| int_arg("preg_match_all", value))
        .transpose()?
        .unwrap_or(0);
    validate_preg_offset_min("preg_match_all", offset)?;
    let subject_bytes = subject.as_bytes();
    let Some(start_offset) = preg_match_offset(subject_bytes.len(), offset) else {
        context.set_preg_last_error(
            pcre::PREG_INTERNAL_ERROR,
            pcre::preg_error_message(pcre::PREG_INTERNAL_ERROR),
        );
        assign_reference_arg(args.get(2), Value::packed_array(Vec::new()));
        return Ok(Value::Bool(false));
    };
    let Some(compiled) = compile_preg_pattern(context, "preg_match_all", pattern, span) else {
        return Ok(Value::Bool(false));
    };
    let match_options = match context.pcre_cache().match_options_for_subject_at_offset(
        &compiled,
        &subject,
        start_offset,
    ) {
        Ok(options) => options,
        Err(error) => {
            assign_reference_arg(args.get(2), Value::packed_array(Vec::new()));
            return preg_failure(context, error);
        }
    };

    let set_order = flags & pcre::PREG_SET_ORDER != 0;
    let mut all = Vec::new();
    if let Err(error) = compiled.for_each_php_match_with_options(
        subject_bytes,
        start_offset,
        match_options,
        |captures| {
            all.push(pcre::captures_to_array_with_names_for_order(
                &captures,
                compiled.capture_names(),
                flags,
                0,
                set_order,
            ));
            Ok(true)
        },
        std::convert::identity,
    ) {
        return preg_failure(context, error);
    }
    let count = all.len() as i64;
    let output = if set_order {
        Value::packed_array(all)
    } else {
        pattern_order_matches(all, compiled.capture_names())
    };
    assign_reference_arg(args.get(2), output);
    context.clear_preg_last_error();
    Ok(Value::Int(count))
}

fn preg_match_offset(subject_len: usize, offset: i64) -> Option<usize> {
    if offset >= 0 {
        let offset = offset as usize;
        return (offset <= subject_len).then_some(offset);
    }
    Some(subject_len.saturating_sub(offset.unsigned_abs() as usize))
}

fn validate_preg_offset_min(function: &str, offset: i64) -> Result<(), BuiltinError> {
    if offset == i64::MIN {
        return Err(argument_value_error(
            function,
            "#5 ($offset)",
            &format!("must be greater than {}", i64::MIN),
        ));
    }
    Ok(())
}

fn validate_preg_match_flags(
    function: &str,
    argument: &str,
    flags: i64,
) -> Result<(), BuiltinError> {
    const VALID_FLAGS: i64 = pcre::PREG_OFFSET_CAPTURE | pcre::PREG_UNMATCHED_AS_NULL;
    if flags & !VALID_FLAGS != 0 {
        return Err(argument_value_error(
            function,
            argument,
            "must be a PREG_* constant",
        ));
    }
    Ok(())
}

fn validate_preg_match_all_flags(flags: i64) -> Result<(), BuiltinError> {
    const VALID_FLAGS: i64 = pcre::PREG_PATTERN_ORDER
        | pcre::PREG_SET_ORDER
        | pcre::PREG_OFFSET_CAPTURE
        | pcre::PREG_UNMATCHED_AS_NULL;
    let order_flags = flags & (pcre::PREG_PATTERN_ORDER | pcre::PREG_SET_ORDER);
    if flags & !VALID_FLAGS != 0 || order_flags == (pcre::PREG_PATTERN_ORDER | pcre::PREG_SET_ORDER)
    {
        return Err(argument_value_error(
            "preg_match_all",
            "#4 ($flags)",
            "must be a PREG_* constant",
        ));
    }
    Ok(())
}
fn preg_replace(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 3 || args.len() > 5 {
        return Err(arity_error("preg_replace", "three to five argument(s)"));
    }
    let limit = args
        .get(3)
        .map(|value| int_arg("preg_replace", value))
        .transpose()?
        .unwrap_or(-1);
    if let Some(result) = preg_replace_simple_literal_scalar(context, &args, limit)? {
        return Ok(result);
    }
    let Some(specs) = preg_replace_specs(context, "preg_replace", &args[0], &args[1], span)? else {
        return Ok(Value::Null);
    };
    let mut count = 0;
    let result = match preg_replace_subject_with_specs(&specs, &args[2], limit, &mut count) {
        Ok(result) => result,
        Err(error) => return preg_replace_failure(context, error),
    };
    assign_reference_arg(args.get(4), Value::Int(count));
    context.clear_preg_last_error();
    Ok(result)
}

fn preg_replace_simple_literal_scalar(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: &[Value],
    limit: i64,
) -> Result<Option<Value>, BuiltinError> {
    let pattern = match deref_value(&args[0]) {
        Value::String(pattern) => pattern,
        _ => return Ok(None),
    };
    let replacement = match deref_value(&args[1]) {
        Value::String(replacement) => replacement,
        _ => return Ok(None),
    };
    let subject = match deref_value(&args[2]) {
        Value::String(subject) => subject,
        _ => return Ok(None),
    };
    if replacement
        .as_bytes()
        .iter()
        .any(|byte| matches!(*byte, b'$' | b'\\'))
    {
        return Ok(None);
    }
    let Ok(Some(literal)) = pcre::simple_literal_pattern(&pattern) else {
        return Ok(None);
    };
    let mut count = 0i64;
    let replaced = replace_literal_bytes(
        subject.as_bytes(),
        literal.as_bytes(),
        replacement.as_bytes(),
        limit,
        &mut count,
    );
    assign_reference_arg(args.get(4), Value::Int(count));
    context.clear_preg_last_error();
    Ok(Some(Value::string(replaced)))
}

fn replace_literal_bytes(
    subject: &[u8],
    needle: &[u8],
    replacement: &[u8],
    limit: i64,
    count: &mut i64,
) -> Vec<u8> {
    let mut output = Vec::with_capacity(subject.len());
    let mut cursor = 0usize;
    while cursor <= subject.len() {
        if limit >= 0 && *count >= limit {
            break;
        }
        let Some((start, end)) = find_literal_match(subject, needle, cursor) else {
            break;
        };
        output.extend_from_slice(&subject[cursor..start]);
        output.extend_from_slice(replacement);
        cursor = end;
        *count += 1;
    }
    output.extend_from_slice(&subject[cursor..]);
    output
}

fn preg_filter(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 3 || args.len() > 5 {
        return Err(arity_error("preg_filter", "three to five argument(s)"));
    }
    let limit = args
        .get(3)
        .map(|value| int_arg("preg_filter", value))
        .transpose()?
        .unwrap_or(-1);
    let Some(specs) = preg_replace_specs(context, "preg_filter", &args[0], &args[1], span)? else {
        return Ok(Value::Null);
    };
    let mut count = 0;
    let result = match preg_replace_filter_subject_with_specs(&specs, &args[2], limit, &mut count) {
        Ok(result) => result,
        Err(error) => return preg_replace_failure(context, error),
    };
    assign_reference_arg(args.get(4), Value::Int(count));
    context.clear_preg_last_error();
    Ok(result)
}

fn preg_replace_specs(
    context: &mut PcreBuiltinServices<'_, '_>,
    function_name: &str,
    pattern: &Value,
    replacement: &Value,
    span: RuntimeSourceSpan,
) -> Result<Option<Vec<PregReplaceSpec>>, BuiltinError> {
    let replacement_array = match deref_value(replacement) {
        Value::Array(array) => Some(array),
        _ => None,
    };

    let patterns = match deref_value(pattern) {
        Value::Array(array) => {
            let mut patterns = Vec::new();
            for (_, value) in array.iter() {
                patterns.push(string_arg(function_name, value)?);
            }
            patterns
        }
        _ if replacement_array.is_some() => {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_BUILTIN_TYPE",
                format!(
                    "{function_name}(): Argument #1 ($pattern) must be of type array when argument #2 ($replacement) is an array, {} given",
                    php_argument_type_name(pattern)
                ),
            ));
        }
        _ => vec![string_arg(function_name, pattern).map_err(|_| {
            argument_type_error(function_name, "#1 ($pattern)", "array|string", pattern)
        })?],
    };

    let replacements = if let Some(array) = replacement_array {
        let mut replacements = Vec::new();
        for (_, value) in array.iter() {
            replacements.push(string_arg(function_name, value)?.into_bytes());
        }
        PregReplaceReplacements::Array(replacements)
    } else {
        PregReplaceReplacements::Scalar(
            string_arg(function_name, replacement)
                .map_err(|_| {
                    argument_type_error(
                        function_name,
                        "#2 ($replacement)",
                        "array|string",
                        replacement,
                    )
                })?
                .into_bytes(),
        )
    };

    let mut specs = Vec::new();
    for (index, pattern) in patterns.into_iter().enumerate() {
        let Some(compiled) = compile_preg_pattern(context, function_name, pattern, span.clone())
        else {
            return Ok(None);
        };
        let replacement = replacements.get(index).to_vec();
        specs.push((compiled, replacement));
    }
    Ok(Some(specs))
}

enum PregReplaceReplacements {
    Scalar(Vec<u8>),
    Array(Vec<Vec<u8>>),
}

impl PregReplaceReplacements {
    fn get(&self, index: usize) -> &[u8] {
        match self {
            Self::Scalar(value) => value,
            Self::Array(values) => values.get(index).map_or(b"".as_slice(), Vec::as_slice),
        }
    }
}
fn preg_replace_callback(
    context: &mut PcreCallbackServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 3 || args.len() > 6 {
        return Err(arity_error(
            "preg_replace_callback",
            "three to six argument(s)",
        ));
    }
    let pattern = string_arg("preg_replace_callback", &args[0])?;
    let limit = args
        .get(3)
        .map(|value| int_arg("preg_replace_callback", value))
        .transpose()?
        .unwrap_or(-1);
    let flags = args
        .get(5)
        .map(|value| int_arg("preg_replace_callback", value))
        .transpose()?
        .unwrap_or(0);
    let callback_name = match deref_value(&args[1]).as_callable() {
        Some(CallableValue::InternalBuiltin { name }) => name.clone(),
        _ => {
            return Err(BuiltinError::new(
                "E_PHP_RUNTIME_CALLABLE_CONTEXT_REQUIRED",
                "preg_replace_callback requires VM callable dispatch for user callbacks",
            ));
        }
    };
    let Some(callback) = BuiltinRegistry::new().get(&callback_name) else {
        return Err(BuiltinError::new(
            "E_PHP_RUNTIME_UNDEFINED_CALLBACK",
            format!("Undefined callback `{callback_name}`"),
        ));
    };
    let Some(compiled) =
        compile_preg_pattern(context, "preg_replace_callback", pattern, span.clone())
    else {
        return Ok(Value::Null);
    };
    let mut count = 0;
    let result = preg_replace_callback_subject(
        context, &compiled, callback, &args[2], limit, flags, &mut count, span,
    )?;
    if matches!(result, Value::Null) && context.preg_last_error().0 != pcre::PREG_NO_ERROR {
        assign_reference_arg(args.get(4), Value::Int(count));
        return Ok(Value::Null);
    }
    assign_reference_arg(args.get(4), Value::Int(count));
    context.clear_preg_last_error();
    Ok(result)
}

pub(in crate::builtins::modules) fn builtin_preg_replace_callback_array(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 6 {
        return Err(arity_error(
            "preg_replace_callback_array",
            "two to six argument(s)",
        ));
    }
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_CALLABLE_CONTEXT_REQUIRED",
        "preg_replace_callback_array requires VM callable dispatch for user callbacks",
    ))
}

fn preg_split(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 4 {
        return Err(arity_error("preg_split", "two to four argument(s)"));
    }
    let pattern = string_arg("preg_split", &args[0])?;
    let subject = string_arg("preg_split", &args[1])?;
    let limit = args
        .get(2)
        .map(|value| int_arg("preg_split", value))
        .transpose()?
        .unwrap_or(-1);
    let flags = args
        .get(3)
        .map(|value| int_arg("preg_split", value))
        .transpose()?
        .unwrap_or(0);
    if flags & pcre::PREG_SPLIT_DELIM_CAPTURE == 0
        && let Ok(Some(literal)) = pcre::simple_literal_pattern(&pattern)
    {
        return preg_split_literal(
            context,
            subject.as_bytes(),
            literal.as_bytes(),
            limit,
            flags,
        );
    }
    let Some(compiled) = compile_preg_pattern(context, "preg_split", pattern, span) else {
        return Ok(Value::Bool(false));
    };
    if let Err(error) = context
        .pcre_cache()
        .validate_utf8_subject_for_pattern(&compiled, &subject)
    {
        return preg_failure(context, error);
    }
    let subject_bytes = subject.as_bytes();
    let can_match_empty = match compiled.is_match(b"") {
        Ok(can_match_empty) => can_match_empty,
        Err(error) => return preg_failure(context, error),
    };
    if !can_match_empty {
        match compiled.captures_at(subject_bytes, 0) {
            Ok(Some(captures)) => {
                if captures
                    .get(0)
                    .is_some_and(|full| full.start() != full.end())
                {
                    return preg_split_non_empty_matches(
                        context,
                        &compiled,
                        subject_bytes,
                        limit,
                        flags,
                    );
                }
            }
            Ok(None) => {
                let mut pieces = PhpArray::new();
                append_split_piece(&mut pieces, subject_bytes, 0, flags);
                context.clear_preg_last_error();
                return Ok(Value::Array(pieces));
            }
            Err(error) => return preg_failure(context, error),
        }
    }
    preg_split_with_empty_matches(context, &compiled, subject_bytes, limit, flags)
}

fn preg_split_non_empty_matches(
    context: &mut PcreBuiltinServices<'_, '_>,
    compiled: &pcre::CompiledPattern,
    subject_bytes: &[u8],
    limit: i64,
    flags: i64,
) -> BuiltinResult {
    let mut pieces = PhpArray::new();
    let mut last_end = 0usize;
    let mut emitted = 0i64;
    for captures in compiled.captures_iter(subject_bytes) {
        let captures = match captures {
            Ok(captures) => captures,
            Err(error) => return preg_failure(context, error.into()),
        };
        let Some(full) = captures.get(0) else {
            continue;
        };
        if limit > 0 && emitted >= limit - 1 {
            break;
        }
        append_split_piece(
            &mut pieces,
            &subject_bytes[last_end..full.start()],
            last_end,
            flags,
        );
        emitted += 1;
        if flags & pcre::PREG_SPLIT_DELIM_CAPTURE != 0 {
            for index in 1..captures.len() {
                if let Some(capture) = captures.get(index) {
                    append_split_piece(&mut pieces, capture.as_bytes(), capture.start(), flags);
                }
            }
        }
        last_end = full.end();
    }
    append_split_piece(&mut pieces, &subject_bytes[last_end..], last_end, flags);
    context.clear_preg_last_error();
    Ok(Value::Array(pieces))
}

fn preg_split_literal(
    context: &mut PcreBuiltinServices<'_, '_>,
    subject_bytes: &[u8],
    needle: &[u8],
    limit: i64,
    flags: i64,
) -> BuiltinResult {
    let mut pieces = PhpArray::new();
    let mut last_end = 0usize;
    let mut emitted = 0i64;
    while last_end <= subject_bytes.len() {
        if limit > 0 && emitted >= limit - 1 {
            break;
        }
        let Some((start, end)) = find_literal_match(subject_bytes, needle, last_end) else {
            break;
        };
        append_split_piece(
            &mut pieces,
            &subject_bytes[last_end..start],
            last_end,
            flags,
        );
        last_end = end;
        emitted += 1;
    }
    append_split_piece(&mut pieces, &subject_bytes[last_end..], last_end, flags);
    context.clear_preg_last_error();
    Ok(Value::Array(pieces))
}

fn preg_split_with_empty_matches(
    context: &mut PcreBuiltinServices<'_, '_>,
    compiled: &pcre::CompiledPattern,
    subject_bytes: &[u8],
    limit: i64,
    flags: i64,
) -> BuiltinResult {
    let mut pieces = PhpArray::new();
    let mut last_end = 0usize;
    let mut search_start = 0usize;
    let mut retry_after_empty_match = false;
    let mut retry_allows_start_reset = false;
    let mut emitted = 0i64;

    while search_start <= subject_bytes.len() {
        let captures = if retry_after_empty_match {
            let mut options = MatchOptions::default().not_empty_at_start(true);
            if !retry_allows_start_reset {
                options = options.anchored(true);
            }
            match compiled.captures_at_with_options(subject_bytes, search_start, options) {
                Ok(Some(captures)) => Some(captures),
                Ok(None) => {
                    retry_after_empty_match = false;
                    search_start = next_split_search_offset(
                        subject_bytes,
                        search_start,
                        compiled.is_utf8_mode(),
                    );
                    continue;
                }
                Err(error) => return preg_failure(context, error),
            }
        } else {
            match compiled.captures_at(subject_bytes, search_start) {
                Ok(captures) => captures,
                Err(error) => return preg_failure(context, error),
            }
        };
        let Some(captures) = captures else {
            break;
        };
        let Some(full) = captures.get(0) else {
            continue;
        };
        if limit > 0 && emitted >= limit - 1 {
            break;
        }
        if full.start() < last_end {
            return preg_failure(
                context,
                pcre::PcreFailure::new(
                    pcre::PREG_INTERNAL_ERROR,
                    "PCRE split match moved before the previous delimiter",
                ),
            );
        }
        let match_start = full.start();
        append_split_piece(
            &mut pieces,
            &subject_bytes[last_end..full.start()],
            last_end,
            flags,
        );
        emitted += 1;
        if flags & pcre::PREG_SPLIT_DELIM_CAPTURE != 0 {
            for index in 1..captures.len() {
                if let Some(capture) = captures.get(index) {
                    append_split_piece(&mut pieces, capture.as_bytes(), capture.start(), flags);
                }
            }
        }
        retry_after_empty_match = full.start() == full.end();
        retry_allows_start_reset = retry_after_empty_match && match_start > search_start;
        search_start = full.end();
        last_end = full.end();
    }
    append_split_piece(&mut pieces, &subject_bytes[last_end..], last_end, flags);
    context.clear_preg_last_error();
    Ok(Value::Array(pieces))
}

fn next_split_search_offset(subject: &[u8], offset: usize, utf8_mode: bool) -> usize {
    if offset >= subject.len() {
        return subject.len() + 1;
    }
    if !utf8_mode {
        return offset + 1;
    }
    std::str::from_utf8(&subject[offset..])
        .ok()
        .and_then(|rest| rest.chars().next())
        .map_or(offset + 1, |character| offset + character.len_utf8())
}

fn preg_grep(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("preg_grep", "two to three argument(s)"));
    }
    let pattern = string_arg("preg_grep", &args[0])?;
    let flags = args
        .get(2)
        .map(|value| int_arg("preg_grep", value))
        .transpose()?
        .unwrap_or(0);
    if let Ok(Some(literal)) = pcre::simple_literal_pattern(&pattern) {
        let Value::Array(input) = deref_value(&args[1]) else {
            return Err(type_error("preg_grep", "array", &args[1]));
        };
        let mut output = PhpArray::new();
        for (key, value) in input.iter() {
            let text = context
                .string_cast_value(value, span.clone())
                .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_TYPE_ERROR", message))?;
            let is_match = find_literal_match(text.as_bytes(), literal.as_bytes(), 0).is_some();
            if is_match != (flags & pcre::PREG_GREP_INVERT != 0) {
                output.insert(key.clone(), value.clone());
            }
        }
        context.clear_preg_last_error();
        return Ok(Value::Array(output));
    }
    let Some(compiled) = compile_preg_pattern(context, "preg_grep", pattern, span.clone()) else {
        return Ok(Value::Bool(false));
    };
    let Value::Array(input) = deref_value(&args[1]) else {
        return Err(type_error("preg_grep", "array", &args[1]));
    };
    let mut output = PhpArray::new();
    for (key, value) in input.iter() {
        let text = context
            .string_cast_value(value, span.clone())
            .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_TYPE_ERROR", message))?;
        if let Err(error) = context
            .pcre_cache()
            .validate_utf8_subject_for_pattern(&compiled, &text)
        {
            return preg_failure(context, error);
        }
        let is_match = match compiled.is_match(text.as_bytes()) {
            Ok(is_match) => is_match,
            Err(error) => return preg_failure(context, error),
        };
        if is_match != (flags & pcre::PREG_GREP_INVERT != 0) {
            output.insert(key.clone(), value.clone());
        }
    }
    context.clear_preg_last_error();
    Ok(Value::Array(output))
}
#[doc(hidden)]
pub fn exact_preg_quote(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(arity_error("preg_quote", "one or two argument(s)"));
    }
    let text = string_arg("preg_quote", &args[0])?;
    let delimiter = args
        .get(1)
        .map(|value| string_arg("preg_quote", value))
        .transpose()?
        .and_then(|delimiter| delimiter.as_bytes().first().copied());
    Ok(Value::string(pcre::preg_quote(text.as_bytes(), delimiter)))
}
fn preg_last_error(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("preg_last_error", &args, 0)?;
    Ok(Value::Int(context.preg_last_error().0))
}
fn preg_last_error_msg(
    context: &mut PcreBuiltinServices<'_, '_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("preg_last_error_msg", &args, 0)?;
    Ok(Value::string(context.preg_last_error().1))
}

fn preg_replace_failure(
    context: &mut PcreBuiltinServices<'_, '_>,
    error: pcre::PcreFailure,
) -> BuiltinResult {
    context.set_preg_last_error(error.code(), pcre::preg_error_message(error.code()));
    Ok(Value::Null)
}
