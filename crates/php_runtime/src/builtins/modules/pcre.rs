//! Pcre builtin registry slice.

use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinRegistry,
    BuiltinResult, RuntimeSourceSpan,
};
use crate::{CallableValue, PhpArray, PhpString, Value, pcre};
use pcre2::bytes::MatchOptions;
use std::sync::Arc;

type PregReplaceSpec = (Arc<pcre::CompiledPattern>, Vec<u8>);

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "preg_filter",
        builtin_preg_filter,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("preg_grep", builtin_preg_grep, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "preg_last_error",
        builtin_preg_last_error,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "preg_last_error_msg",
        builtin_preg_last_error_msg,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("preg_match", builtin_preg_match, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "preg_match_all",
        builtin_preg_match_all,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("preg_quote", builtin_preg_quote, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "preg_replace",
        builtin_preg_replace,
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
    BuiltinEntry::new("preg_split", builtin_preg_split, BuiltinCompatibility::Php),
];

pub(in crate::builtins::modules) fn builtin_preg_match(
    context: &mut BuiltinContext<'_>,
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

pub(in crate::builtins::modules) fn builtin_preg_match_all(
    context: &mut BuiltinContext<'_>,
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
pub(in crate::builtins::modules) fn builtin_preg_replace(
    context: &mut BuiltinContext<'_>,
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

pub(in crate::builtins::modules) fn builtin_preg_filter(
    context: &mut BuiltinContext<'_>,
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
    context: &mut BuiltinContext<'_>,
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
pub(in crate::builtins::modules) fn builtin_preg_replace_callback(
    context: &mut BuiltinContext<'_>,
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

pub(in crate::builtins::modules) fn builtin_preg_split(
    context: &mut BuiltinContext<'_>,
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
    context: &mut BuiltinContext<'_>,
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

fn preg_split_with_empty_matches(
    context: &mut BuiltinContext<'_>,
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

pub(in crate::builtins::modules) fn builtin_preg_grep(
    context: &mut BuiltinContext<'_>,
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
    let Some(compiled) = compile_preg_pattern(context, "preg_grep", pattern, span.clone()) else {
        return Ok(Value::Bool(false));
    };
    let Value::Array(input) = deref_value(&args[1]) else {
        return Err(type_error("preg_grep", "array", &args[1]));
    };
    let mut output = PhpArray::new();
    for (key, value) in input.iter() {
        let text = string_cast_value(context, value, span.clone())
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
pub(in crate::builtins::modules) fn builtin_preg_quote(
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
pub(in crate::builtins::modules) fn builtin_preg_last_error(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("preg_last_error", &args, 0)?;
    Ok(Value::Int(context.preg_last_error().0))
}
pub(in crate::builtins::modules) fn builtin_preg_last_error_msg(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("preg_last_error_msg", &args, 0)?;
    Ok(Value::string(context.preg_last_error().1))
}

fn preg_replace_failure(
    context: &mut BuiltinContext<'_>,
    error: pcre::PcreFailure,
) -> BuiltinResult {
    context.set_preg_last_error(error.code(), pcre::preg_error_message(error.code()));
    Ok(Value::Null)
}
