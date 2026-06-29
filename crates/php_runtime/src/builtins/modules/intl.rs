//! Bounded intl builtin implementations.

use super::core::{expect_arity, int_arg, string_arg};
use crate::Value;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "grapheme_strlen",
        builtin_grapheme_strlen,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "intl_get_error_code",
        builtin_intl_get_error_code,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "normalizer_normalize",
        builtin_normalizer_normalize,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "normalizer_is_normalized",
        builtin_normalizer_is_normalized,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "grapheme_substr",
        builtin_grapheme_substr,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "transliterator_transliterate",
        builtin_transliterator_transliterate,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_grapheme_strlen(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("grapheme_strlen", &args, 1)?;
    let input = utf8_string_arg("grapheme_strlen", &args[0])?;
    Ok(Value::Int(input.chars().count() as i64))
}

fn builtin_intl_get_error_code(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("intl_get_error_code", &args, 0)?;
    Ok(Value::Int(0))
}

fn builtin_normalizer_normalize(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(super::core::arity_error(
            "normalizer_normalize",
            "one or two argument(s)",
        ));
    }
    let input = utf8_string_arg("normalizer_normalize", &args[0])?;
    let form = args
        .get(1)
        .map(|value| int_arg("normalizer_normalize", value))
        .transpose()?
        .unwrap_or(NORMALIZER_FORM_C);
    if form != NORMALIZER_FORM_C {
        return Err(unsupported_intl(
            "normalizer_normalize",
            "only NFC form is supported",
        ));
    }
    Ok(Value::string(input.into_bytes()))
}

fn builtin_normalizer_is_normalized(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(super::core::arity_error(
            "normalizer_is_normalized",
            "one or two argument(s)",
        ));
    }
    let _ = utf8_string_arg("normalizer_is_normalized", &args[0])?;
    let form = args
        .get(1)
        .map(|value| int_arg("normalizer_is_normalized", value))
        .transpose()?
        .unwrap_or(NORMALIZER_FORM_C);
    if form != NORMALIZER_FORM_C {
        return Err(unsupported_intl(
            "normalizer_is_normalized",
            "only NFC form is supported",
        ));
    }
    Ok(Value::Bool(true))
}

fn builtin_grapheme_substr(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(super::core::arity_error(
            "grapheme_substr",
            "two or three argument(s)",
        ));
    }
    let input = utf8_string_arg("grapheme_substr", &args[0])?;
    let start = int_arg("grapheme_substr", &args[1])?;
    let chars = input.chars().collect::<Vec<_>>();
    let len = chars.len() as i64;
    let start = if start < 0 { len + start } else { start }.clamp(0, len);
    let count = args
        .get(2)
        .map(|value| int_arg("grapheme_substr", value))
        .transpose()?
        .unwrap_or(len - start);
    if count < 0 {
        return Ok(Value::string(Vec::<u8>::new()));
    }
    let end = (start + count).clamp(start, len);
    Ok(Value::string(
        chars[start as usize..end as usize]
            .iter()
            .collect::<String>()
            .into_bytes(),
    ))
}

fn builtin_transliterator_transliterate(
    _context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("transliterator_transliterate", &args, 2)?;
    let id = utf8_string_arg("transliterator_transliterate", &args[0])?;
    let input = utf8_string_arg("transliterator_transliterate", &args[1])?;
    if !matches!(
        id.as_str(),
        "Any-Latin; Latin-ASCII" | "Latin-ASCII" | "Any-Latin"
    ) {
        return Err(unsupported_intl(
            "transliterator_transliterate",
            "only simple Latin ASCII transliteration is supported",
        ));
    }
    Ok(Value::string(
        transliterate_latin_ascii(&input).into_bytes(),
    ))
}

pub const NORMALIZER_FORM_C: i64 = 4;

fn utf8_string_arg(name: &str, value: &Value) -> Result<String, BuiltinError> {
    let input = string_arg(name, value)?;
    std::str::from_utf8(input.as_bytes())
        .map(str::to_owned)
        .map_err(|_| {
            BuiltinError::new(
                "E_PHP_RUNTIME_INTL_UTF8",
                format!("{name}(): input must be valid UTF-8"),
            )
        })
}

fn unsupported_intl(name: &'static str, detail: &'static str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_UNSUPPORTED_INTL",
        format!("{name}(): {detail}"),
    )
}

fn transliterate_latin_ascii(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' => 'a',
            'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' | 'Ā' => 'A',
            'é' | 'è' | 'ê' | 'ë' | 'ē' => 'e',
            'É' | 'È' | 'Ê' | 'Ë' | 'Ē' => 'E',
            'í' | 'ì' | 'î' | 'ï' | 'ī' => 'i',
            'Í' | 'Ì' | 'Î' | 'Ï' | 'Ī' => 'I',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ō' => 'o',
            'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' | 'Ō' => 'O',
            'ú' | 'ù' | 'û' | 'ü' | 'ū' => 'u',
            'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ū' => 'U',
            'ñ' => 'n',
            'Ñ' => 'N',
            'ç' => 'c',
            'Ç' => 'C',
            'ß' => 's',
            _ => ch,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;
    use crate::builtins::BuiltinRegistry;

    #[test]
    fn grapheme_strlen_counts_utf8_chars() {
        let entry = BuiltinRegistry::new()
            .get("grapheme_strlen")
            .expect("grapheme_strlen exists");
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let value = (entry.function())(
            &mut context,
            vec![Value::string("hé".as_bytes().to_vec())],
            RuntimeSourceSpan::default(),
        )
        .expect("grapheme_strlen succeeds");

        assert_eq!(value, Value::Int(2));
    }
}
