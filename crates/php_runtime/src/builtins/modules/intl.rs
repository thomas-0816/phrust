//! Bounded intl builtin implementations.

use super::core::{expect_arity, int_arg, string_arg};
use crate::Value;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinError, BuiltinResult,
    RuntimeSourceSpan,
};
use unicode_normalization::{UnicodeNormalization, is_nfc, is_nfd, is_nfkc, is_nfkd};
use unicode_segmentation::UnicodeSegmentation;

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
        "intl_get_error_message",
        builtin_intl_get_error_message,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "locale_get_primary_language",
        builtin_locale_get_primary_language,
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
        "grapheme_strpos",
        builtin_grapheme_strpos,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "grapheme_stripos",
        builtin_grapheme_stripos,
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
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("grapheme_strlen", &args, 1)?;
    let input = utf8_string_arg("grapheme_strlen", &args[0])?;
    Ok(Value::Int(graphemes(&input).len() as i64))
}

fn builtin_intl_get_error_message(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("intl_get_error_message", &args, 0)?;
    Ok(Value::string("U_ZERO_ERROR"))
}

pub fn primary_language(locale: &str) -> String {
    let locale = locale.split('@').next().unwrap_or(locale);
    let subtags = locale.split(['-', '_']).collect::<Vec<_>>();
    if subtags.len() >= 3 && subtags[0].eq_ignore_ascii_case("zh") && subtags[1] == "min" {
        return "zh".to_owned();
    }
    if subtags.len() >= 2
        && matches!(subtags[0].to_ascii_lowercase().as_str(), "i" | "zh" | "sgn")
        && subtags[1].chars().all(|ch| ch.is_ascii_lowercase())
    {
        return format!("{}-{}", subtags[0], subtags[1]).to_ascii_lowercase();
    }
    subtags
        .first()
        .copied()
        .unwrap_or(locale)
        .to_ascii_lowercase()
}

fn builtin_locale_get_primary_language(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("locale_get_primary_language", &args, 1)?;
    let locale = utf8_string_arg("locale_get_primary_language", &args[0])?;
    Ok(Value::string(primary_language(&locale)))
}

fn builtin_intl_get_error_code(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    expect_arity("intl_get_error_code", &args, 0)?;
    Ok(Value::Int(0))
}

fn builtin_normalizer_normalize(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
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
    normalize_string(&input, form)
        .map(|value| Value::string(value.into_bytes()))
        .ok_or_else(|| invalid_normalizer_form("normalizer_normalize"))
}

fn builtin_normalizer_is_normalized(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 2 {
        return Err(super::core::arity_error(
            "normalizer_is_normalized",
            "one or two argument(s)",
        ));
    }
    let input = utf8_string_arg("normalizer_is_normalized", &args[0])?;
    let form = args
        .get(1)
        .map(|value| int_arg("normalizer_is_normalized", value))
        .transpose()?
        .unwrap_or(NORMALIZER_FORM_C);
    is_normalized_string(&input, form)
        .map(Value::Bool)
        .ok_or_else(|| invalid_normalizer_form("normalizer_is_normalized"))
}

fn builtin_grapheme_substr(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
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
    let clusters = graphemes(&input);
    let len = clusters.len() as i64;
    let start = if start < 0 { len + start } else { start }.clamp(0, len);
    let count = args
        .get(2)
        .map(|value| int_arg("grapheme_substr", value))
        .transpose()?
        .unwrap_or(len - start);
    let end = if count < 0 {
        (len + count).clamp(start, len)
    } else {
        (start + count).clamp(start, len)
    };
    Ok(Value::string(
        clusters[start as usize..end as usize].concat(),
    ))
}

fn builtin_grapheme_strpos(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    grapheme_strpos_impl("grapheme_strpos", args, false)
}

fn builtin_grapheme_stripos(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    grapheme_strpos_impl("grapheme_stripos", args, true)
}

fn builtin_transliterator_transliterate(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
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

pub const NORMALIZER_FORM_D: i64 = 4;
pub const NORMALIZER_FORM_KD: i64 = 8;
pub const NORMALIZER_FORM_C: i64 = 16;
pub const NORMALIZER_FORM_KC: i64 = 32;

pub fn normalize_string(input: &str, form: i64) -> Option<String> {
    match form {
        NORMALIZER_FORM_D => Some(input.nfd().collect()),
        NORMALIZER_FORM_KD => Some(input.nfkd().collect()),
        NORMALIZER_FORM_C => Some(input.nfc().collect()),
        NORMALIZER_FORM_KC => Some(input.nfkc().collect()),
        _ => None,
    }
}

pub fn is_normalized_string(input: &str, form: i64) -> Option<bool> {
    match form {
        NORMALIZER_FORM_D => Some(is_nfd(input)),
        NORMALIZER_FORM_KD => Some(is_nfkd(input)),
        NORMALIZER_FORM_C => Some(is_nfc(input)),
        NORMALIZER_FORM_KC => Some(is_nfkc(input)),
        _ => None,
    }
}

fn graphemes(input: &str) -> Vec<&str> {
    UnicodeSegmentation::graphemes(input, true).collect()
}

fn grapheme_strpos_impl(
    function: &'static str,
    args: crate::builtins::BuiltinArgs,
    case_insensitive: bool,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(super::core::arity_error(
            function,
            "two or three argument(s)",
        ));
    }
    let haystack = utf8_string_arg(function, &args[0])?;
    let needle = utf8_string_arg(function, &args[1])?;
    let offset = args
        .get(2)
        .map(|value| int_arg(function, value))
        .transpose()?
        .unwrap_or(0);
    let haystack_clusters = graphemes(&haystack);
    let len = haystack_clusters.len() as i64;
    let start = if offset < 0 { len + offset } else { offset };
    if start < 0 || start > len {
        return Ok(Value::Bool(false));
    }
    if needle.is_empty() {
        return Ok(Value::Int(start));
    }
    let search_haystack = if case_insensitive {
        haystack.to_lowercase()
    } else {
        haystack.clone()
    };
    let search_needle = if case_insensitive {
        needle.to_lowercase()
    } else {
        needle
    };
    let byte_start = haystack_clusters
        .iter()
        .take(start as usize)
        .map(|cluster| cluster.len())
        .sum::<usize>();
    let Some(relative_byte) = search_haystack[byte_start..].find(&search_needle) else {
        return Ok(Value::Bool(false));
    };
    let absolute_byte = byte_start + relative_byte;
    let grapheme_index = haystack[..absolute_byte].graphemes(true).count();
    Ok(Value::Int(grapheme_index as i64))
}

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

fn invalid_normalizer_form(name: &'static str) -> BuiltinError {
    BuiltinError::new(
        "E_PHP_RUNTIME_INTL_NORMALIZER_FORM",
        format!("{name}(): Argument #2 ($form) must be a valid normalization form"),
    )
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
