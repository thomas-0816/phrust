//! Pcre builtin registry slice.

use super::core::*;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinRegistry,
    BuiltinResult, RuntimeSourceSpan,
};
use crate::{CallableValue, PhpArray, Value, pcre, to_string};
use std::sync::Arc;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
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
    BuiltinEntry::new("preg_split", builtin_preg_split, BuiltinCompatibility::Php),
];

pub(in crate::builtins::modules) fn builtin_preg_match(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 5 {
        return Err(arity_error("preg_match", "two to five argument(s)"));
    }
    let pattern = string_arg("preg_match", &args[0])?;
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
    let subject_bytes = subject.as_bytes();
    if offset < 0 || offset as usize > subject_bytes.len() {
        context.set_preg_last_error(
            pcre::PREG_BAD_UTF8_OFFSET_ERROR,
            pcre::preg_error_message(pcre::PREG_BAD_UTF8_OFFSET_ERROR),
        );
        return Ok(Value::Bool(false));
    }
    let Some(compiled) = compile_preg_pattern(context, pattern) else {
        return Ok(Value::Bool(false));
    };
    match compiled.captures(&subject_bytes[offset as usize..]) {
        Ok(Some(captures)) => {
            let matches = pcre::captures_to_array_with_names(
                &captures,
                compiled.capture_names(),
                flags,
                offset as usize,
            );
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
pub(in crate::builtins::modules) fn builtin_preg_match_all(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 5 {
        return Err(arity_error("preg_match_all", "two to five argument(s)"));
    }
    let pattern = string_arg("preg_match_all", &args[0])?;
    let subject = string_arg("preg_match_all", &args[1])?;
    let flags = args
        .get(3)
        .map(|value| int_arg("preg_match_all", value))
        .transpose()?
        .unwrap_or(pcre::PREG_PATTERN_ORDER);
    let offset = args
        .get(4)
        .map(|value| int_arg("preg_match_all", value))
        .transpose()?
        .unwrap_or(0);
    let subject_bytes = subject.as_bytes();
    if offset < 0 || offset as usize > subject_bytes.len() {
        context.set_preg_last_error(
            pcre::PREG_BAD_UTF8_OFFSET_ERROR,
            pcre::preg_error_message(pcre::PREG_BAD_UTF8_OFFSET_ERROR),
        );
        return Ok(Value::Bool(false));
    }
    let Some(compiled) = compile_preg_pattern(context, pattern) else {
        return Ok(Value::Bool(false));
    };

    let mut all = Vec::new();
    for captures in compiled.captures_iter(&subject_bytes[offset as usize..]) {
        match captures {
            Ok(captures) => all.push(pcre::captures_to_array_with_names(
                &captures,
                compiled.capture_names(),
                flags,
                offset as usize,
            )),
            Err(error) => return preg_failure(context, error.into()),
        }
    }
    let count = all.len() as i64;
    let output = if flags & pcre::PREG_SET_ORDER != 0 {
        Value::packed_array(all)
    } else {
        pattern_order_matches(all, compiled.capture_names().len())
    };
    assign_reference_arg(args.get(2), output);
    context.clear_preg_last_error();
    Ok(Value::Int(count))
}
pub(in crate::builtins::modules) fn builtin_preg_replace(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 3 || args.len() > 5 {
        return Err(arity_error("preg_replace", "three to five argument(s)"));
    }
    let limit = args
        .get(3)
        .map(|value| int_arg("preg_replace", value))
        .transpose()?
        .unwrap_or(-1);
    let Some(specs) = preg_replace_specs(context, &args[0], &args[1])? else {
        return Ok(Value::Bool(false));
    };
    let mut count = 0;
    let result = match preg_replace_subject_with_specs(&specs, &args[2], limit, &mut count) {
        Ok(result) => result,
        Err(error) => return preg_failure(context, error),
    };
    assign_reference_arg(args.get(4), Value::Int(count));
    context.clear_preg_last_error();
    Ok(result)
}

fn preg_replace_specs(
    context: &mut BuiltinContext<'_>,
    pattern: &Value,
    replacement: &Value,
) -> Result<Option<Vec<(Arc<pcre::CompiledPattern>, Vec<u8>)>>, BuiltinError> {
    let replacement_array = match deref_value(replacement) {
        Value::Array(array) => Some(array),
        _ => None,
    };

    let patterns = match deref_value(pattern) {
        Value::Array(array) => {
            let mut patterns = Vec::new();
            for (_, value) in array.iter() {
                patterns.push(string_arg("preg_replace", value)?);
            }
            patterns
        }
        _ if replacement_array.is_some() => {
            return Err(type_error("preg_replace", "array", pattern));
        }
        _ => vec![string_arg("preg_replace", pattern)?],
    };

    let replacements = if let Some(array) = replacement_array {
        let mut replacements = Vec::new();
        for (_, value) in array.iter() {
            replacements.push(string_arg("preg_replace", value)?.into_bytes());
        }
        PregReplaceReplacements::Array(replacements)
    } else {
        PregReplaceReplacements::Scalar(string_arg("preg_replace", replacement)?.into_bytes())
    };

    let mut specs = Vec::new();
    for (index, pattern) in patterns.into_iter().enumerate() {
        let Some(compiled) = compile_preg_pattern(context, pattern) else {
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
    if args.len() < 3 || args.len() > 5 {
        return Err(arity_error(
            "preg_replace_callback",
            "three to five argument(s)",
        ));
    }
    let pattern = string_arg("preg_replace_callback", &args[0])?;
    let limit = args
        .get(3)
        .map(|value| int_arg("preg_replace_callback", value))
        .transpose()?
        .unwrap_or(-1);
    let callback_name = match deref_value(&args[1]) {
        Value::Callable(CallableValue::InternalBuiltin { name }) => name.clone(),
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
    let Some(compiled) = compile_preg_pattern(context, pattern) else {
        return Ok(Value::Bool(false));
    };
    let mut count = 0;
    let result = preg_replace_callback_subject(
        context, &compiled, callback, &args[2], limit, &mut count, span,
    )?;
    assign_reference_arg(args.get(4), Value::Int(count));
    context.clear_preg_last_error();
    Ok(result)
}
pub(in crate::builtins::modules) fn builtin_preg_split(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
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
    let Some(compiled) = compile_preg_pattern(context, pattern) else {
        return Ok(Value::Bool(false));
    };
    let mut pieces = PhpArray::new();
    let mut last_end = 0usize;
    let mut emitted = 0i64;
    for captures in compiled.captures_iter(subject.as_bytes()) {
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
            &subject.as_bytes()[last_end..full.start()],
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
    append_split_piece(
        &mut pieces,
        &subject.as_bytes()[last_end..],
        last_end,
        flags,
    );
    context.clear_preg_last_error();
    Ok(Value::Array(pieces))
}
pub(in crate::builtins::modules) fn builtin_preg_grep(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
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
    let Some(compiled) = compile_preg_pattern(context, pattern) else {
        return Ok(Value::Bool(false));
    };
    let Value::Array(input) = deref_value(&args[1]) else {
        return Err(type_error("preg_grep", "array", &args[1]));
    };
    let mut output = PhpArray::new();
    for (key, value) in input.iter() {
        let text = to_string(value)
            .map_err(|message| BuiltinError::new("E_PHP_RUNTIME_TYPE_ERROR", message))?;
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
