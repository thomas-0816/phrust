//! gettext/nls extension fallback surface.

use super::core::{argument_value_error, arity_error, int_arg, string_arg, value_error};
use crate::Value;
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, RuntimeSourceSpan,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

const MAX_DOMAIN_LENGTH: usize = 1024;
const MAX_MSGID_LENGTH: usize = 4096;
const LC_ALL: i64 = libc::LC_ALL as i64;
const LC_CTYPE: i64 = libc::LC_CTYPE as i64;
const LC_MESSAGES: i64 = libc::LC_MESSAGES as i64;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new("_", builtin_gettext, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "bind_textdomain_codeset",
        builtin_bind_textdomain_codeset,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "bindtextdomain",
        builtin_bindtextdomain,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("dcgettext", builtin_dcgettext, BuiltinCompatibility::Php),
    BuiltinEntry::new("dcngettext", builtin_dcngettext, BuiltinCompatibility::Php),
    BuiltinEntry::new("dgettext", builtin_dgettext, BuiltinCompatibility::Php),
    BuiltinEntry::new("dngettext", builtin_dngettext, BuiltinCompatibility::Php),
    BuiltinEntry::new("gettext", builtin_gettext, BuiltinCompatibility::Php),
    BuiltinEntry::new("ngettext", builtin_ngettext, BuiltinCompatibility::Php),
    BuiltinEntry::new("textdomain", builtin_textdomain, BuiltinCompatibility::Php),
];

fn builtin_textdomain(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 1 {
        return Err(arity_error("textdomain", "at most one argument"));
    }
    if args.is_empty() || matches!(args[0], Value::Null) {
        return Ok(Value::string(context.gettext_state().current_domain()));
    }
    let domain = string_arg("textdomain", &args[0])?;
    let domain = domain.to_string_lossy();
    validate_domain("textdomain", "#1 ($domain)", &domain)?;
    if domain == "0" {
        return Err(argument_value_error(
            "textdomain",
            "#1 ($domain)",
            "cannot be zero",
        ));
    }
    Ok(Value::string(
        context.gettext_state().set_domain(domain.to_owned()),
    ))
}

fn builtin_gettext(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("gettext", "exactly one argument"));
    }
    let message = string_arg("gettext", &args[0])?;
    validate_message("gettext", "#1 ($message)", message.as_bytes().len())?;
    let domain = context.gettext_state().current_domain().to_owned();
    Ok(Value::string(translate(
        context,
        &domain,
        message.as_bytes(),
        None,
        1,
        LC_MESSAGES,
    )))
}

fn builtin_dgettext(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("dgettext", "exactly two arguments"));
    }
    let domain = string_arg("dgettext", &args[0])?;
    let message = string_arg("dgettext", &args[1])?;
    validate_domain("dgettext", "#1 ($domain)", &domain.to_string_lossy())?;
    validate_message("dgettext", "#2 ($message)", message.as_bytes().len())?;
    Ok(Value::string(translate(
        context,
        &domain.to_string_lossy(),
        message.as_bytes(),
        None,
        1,
        LC_MESSAGES,
    )))
}

fn builtin_dcgettext(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("dcgettext", "exactly three arguments"));
    }
    let domain = string_arg("dcgettext", &args[0])?;
    let message = string_arg("dcgettext", &args[1])?;
    let category = int_arg("dcgettext", &args[2])?;
    validate_domain("dcgettext", "#1 ($domain)", &domain.to_string_lossy())?;
    validate_message("dcgettext", "#2 ($message)", message.as_bytes().len())?;
    validate_category("dcgettext", "#3 ($category)", category)?;
    Ok(Value::string(translate(
        context,
        &domain.to_string_lossy(),
        message.as_bytes(),
        None,
        1,
        category,
    )))
}

fn builtin_bindtextdomain(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error("bindtextdomain", "one or two arguments"));
    }
    let domain = string_arg("bindtextdomain", &args[0])?;
    let domain = domain.to_string_lossy();
    validate_domain("bindtextdomain", "#1 ($domain)", &domain)?;
    if args.len() == 1 || matches!(args[1], Value::Null) {
        return Ok(context
            .gettext_state()
            .domain_path(&domain)
            .map(Value::string)
            .unwrap_or(Value::Bool(false)));
    }

    let directory = string_arg("bindtextdomain", &args[1])?;
    let directory = directory.to_string_lossy();
    let resolved = if directory.is_empty() || directory == "0" {
        context.cwd().to_path_buf()
    } else {
        let path = PathBuf::from(directory.as_str());
        if !path.exists() {
            return Ok(Value::Bool(false));
        }
        path.canonicalize()
            .map_err(|_| value_error("bindtextdomain", "directory cannot be resolved"))?
    };
    Ok(Value::string(context.gettext_state().bind_domain_path(
        domain.to_owned(),
        resolved.to_string_lossy().into_owned(),
    )))
}

fn builtin_ngettext(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("ngettext", "exactly three arguments"));
    }
    let singular = string_arg("ngettext", &args[0])?;
    let plural = string_arg("ngettext", &args[1])?;
    let count = int_arg("ngettext", &args[2])?;
    validate_message("ngettext", "#1 ($singular)", singular.as_bytes().len())?;
    validate_message("ngettext", "#2 ($plural)", plural.as_bytes().len())?;
    let domain = context.gettext_state().current_domain().to_owned();
    Ok(Value::string(translate(
        context,
        &domain,
        singular.as_bytes(),
        Some(plural.as_bytes()),
        count,
        LC_MESSAGES,
    )))
}

fn builtin_dngettext(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 4 {
        return Err(arity_error("dngettext", "exactly four arguments"));
    }
    let domain = string_arg("dngettext", &args[0])?;
    let singular = string_arg("dngettext", &args[1])?;
    let plural = string_arg("dngettext", &args[2])?;
    let count = int_arg("dngettext", &args[3])?;
    validate_domain("dngettext", "#1 ($domain)", &domain.to_string_lossy())?;
    validate_message("dngettext", "#2 ($singular)", singular.as_bytes().len())?;
    validate_message("dngettext", "#3 ($plural)", plural.as_bytes().len())?;
    Ok(Value::string(translate(
        context,
        &domain.to_string_lossy(),
        singular.as_bytes(),
        Some(plural.as_bytes()),
        count,
        LC_MESSAGES,
    )))
}

fn builtin_dcngettext(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 5 {
        return Err(arity_error("dcngettext", "exactly five arguments"));
    }
    let domain = string_arg("dcngettext", &args[0])?;
    let singular = string_arg("dcngettext", &args[1])?;
    let plural = string_arg("dcngettext", &args[2])?;
    let count = int_arg("dcngettext", &args[3])?;
    let category = int_arg("dcngettext", &args[4])?;
    validate_domain("dcngettext", "#1 ($domain)", &domain.to_string_lossy())?;
    validate_message("dcngettext", "#2 ($singular)", singular.as_bytes().len())?;
    validate_message("dcngettext", "#3 ($plural)", plural.as_bytes().len())?;
    validate_category("dcngettext", "#5 ($category)", category)?;
    Ok(Value::string(translate(
        context,
        &domain.to_string_lossy(),
        singular.as_bytes(),
        Some(plural.as_bytes()),
        count,
        category,
    )))
}

fn builtin_bind_textdomain_codeset(
    context: &mut BuiltinContext<'_>,
    args: Vec<Value>,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if !(1..=2).contains(&args.len()) {
        return Err(arity_error(
            "bind_textdomain_codeset",
            "one or two arguments",
        ));
    }
    let domain = string_arg("bind_textdomain_codeset", &args[0])?;
    let domain = domain.to_string_lossy();
    validate_domain("bind_textdomain_codeset", "#1 ($domain)", &domain)?;
    if args.len() == 1 || matches!(args[1], Value::Null) {
        return Ok(context
            .gettext_state()
            .domain_codeset(&domain)
            .map(Value::string)
            .unwrap_or(Value::Bool(false)));
    }
    let codeset = string_arg("bind_textdomain_codeset", &args[1])?;
    Ok(Value::string(context.gettext_state().bind_domain_codeset(
        domain.to_owned(),
        codeset.to_string_lossy().to_owned(),
    )))
}

fn validate_domain(name: &str, argument: &str, domain: &str) -> Result<(), crate::BuiltinError> {
    if domain.len() > MAX_DOMAIN_LENGTH {
        return Err(argument_value_error(name, argument, "is too long"));
    }
    if domain.as_bytes().contains(&0) {
        return Err(argument_value_error(
            name,
            argument,
            "must not contain any null bytes",
        ));
    }
    if domain.is_empty() {
        return Err(argument_value_error(name, argument, "must not be empty"));
    }
    Ok(())
}

fn validate_message(name: &str, argument: &str, length: usize) -> Result<(), crate::BuiltinError> {
    if length > MAX_MSGID_LENGTH {
        return Err(argument_value_error(name, argument, "is too long"));
    }
    Ok(())
}

fn validate_category(name: &str, argument: &str, category: i64) -> Result<(), crate::BuiltinError> {
    if category == LC_ALL {
        return Err(argument_value_error(name, argument, "cannot be LC_ALL"));
    }
    Ok(())
}

fn translate(
    context: &mut BuiltinContext<'_>,
    domain: &str,
    singular: &[u8],
    plural: Option<&[u8]>,
    count: i64,
    category: i64,
) -> Vec<u8> {
    let Some(catalog) = load_catalog(context, domain, category) else {
        return fallback_plural(singular, plural, count);
    };
    let key = plural.map_or_else(
        || singular.to_vec(),
        |plural| {
            let mut key = singular.to_vec();
            key.push(0);
            key.extend_from_slice(plural);
            key
        },
    );
    let Some(translation) = catalog.messages.get(&key) else {
        return fallback_plural(singular, plural, count);
    };
    if plural.is_none() {
        return translation.clone();
    }
    let forms = translation.split(|byte| *byte == 0).collect::<Vec<_>>();
    let index = catalog
        .plural_index(count)
        .min(forms.len().saturating_sub(1));
    forms
        .get(index)
        .filter(|form| !form.is_empty())
        .map_or_else(
            || fallback_plural(singular, plural, count),
            |form| form.to_vec(),
        )
}

fn fallback_plural(singular: &[u8], plural: Option<&[u8]>, count: i64) -> Vec<u8> {
    if count == 1 {
        singular.to_vec()
    } else {
        plural.unwrap_or(singular).to_vec()
    }
}

fn load_catalog(
    context: &BuiltinContext<'_>,
    domain: &str,
    category: i64,
) -> Option<GettextCatalog> {
    let root = PathBuf::from(context.gettext_state_ref().domain_path(domain)?);
    let locale = gettext_locale(context, category)?;
    for candidate in locale_candidates(&locale) {
        let path = root
            .join(candidate)
            .join(category_name(category))
            .join(format!("{domain}.mo"));
        if let Ok(bytes) = fs::read(path)
            && let Some(catalog) = GettextCatalog::parse(&bytes)
        {
            return Some(catalog);
        }
    }
    None
}

fn gettext_locale(context: &BuiltinContext<'_>, category: i64) -> Option<String> {
    context
        .env_value("LC_ALL")
        .filter(|value| !value.is_empty())
        .or_else(|| {
            if category == LC_MESSAGES {
                context
                    .env_value("LC_MESSAGES")
                    .filter(|value| !value.is_empty())
            } else if category == LC_CTYPE {
                context
                    .env_value("LC_CTYPE")
                    .filter(|value| !value.is_empty())
            } else {
                None
            }
        })
        .or_else(|| context.env_value("LANG").filter(|value| !value.is_empty()))
        .map(ToOwned::to_owned)
}

fn locale_candidates(locale: &str) -> Vec<&str> {
    let trimmed = locale
        .split_once('@')
        .map_or(locale, |(locale, _)| locale)
        .split_once('.')
        .map_or(locale, |(locale, _)| locale);
    if trimmed == locale {
        vec![locale]
    } else {
        vec![locale, trimmed]
    }
}

fn category_name(category: i64) -> &'static str {
    if category == LC_CTYPE {
        "LC_CTYPE"
    } else {
        "LC_MESSAGES"
    }
}

#[derive(Clone, Debug)]
struct GettextCatalog {
    messages: BTreeMap<Vec<u8>, Vec<u8>>,
    plural_expression: PluralExpression,
}

impl GettextCatalog {
    fn parse(bytes: &[u8]) -> Option<Self> {
        let endian = match read_u32_le(bytes, 0)? {
            0x9504_12de => Endian::Little,
            0xde12_0495 => Endian::Big,
            _ => return None,
        };
        let count = read_u32(bytes, 8, endian)? as usize;
        let originals_offset = read_u32(bytes, 12, endian)? as usize;
        let translations_offset = read_u32(bytes, 16, endian)? as usize;
        let mut messages = BTreeMap::new();
        for index in 0..count {
            let original = read_mo_string(bytes, originals_offset + index * 8, endian)?;
            let translation = read_mo_string(bytes, translations_offset + index * 8, endian)?;
            messages.insert(original, translation);
        }
        let plural_expression = messages
            .get(&Vec::new())
            .and_then(|metadata| std::str::from_utf8(metadata).ok())
            .map(PluralExpression::from_metadata)
            .unwrap_or_default();
        Some(Self {
            messages,
            plural_expression,
        })
    }

    fn plural_index(&self, count: i64) -> usize {
        self.plural_expression.index(count)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Endian {
    Little,
    Big,
}

fn read_mo_string(bytes: &[u8], table_offset: usize, endian: Endian) -> Option<Vec<u8>> {
    let len = read_u32(bytes, table_offset, endian)? as usize;
    let offset = read_u32(bytes, table_offset + 4, endian)? as usize;
    bytes.get(offset..offset.checked_add(len)?).map(Vec::from)
}

fn read_u32(bytes: &[u8], offset: usize, endian: Endian) -> Option<u32> {
    let raw = bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(match endian {
        Endian::Little => u32::from_le_bytes(raw),
        Endian::Big => u32::from_be_bytes(raw),
    })
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PluralExpression {
    English,
    French,
    One,
}

impl Default for PluralExpression {
    fn default() -> Self {
        Self::English
    }
}

impl PluralExpression {
    fn from_metadata(metadata: &str) -> Self {
        let normalized = metadata.replace(' ', "");
        if normalized.contains("nplurals=1") {
            Self::One
        } else if normalized.contains("plural=n>1") {
            Self::French
        } else {
            Self::English
        }
    }

    fn index(self, count: i64) -> usize {
        match self {
            Self::One => 0,
            Self::French => usize::from(count > 1),
            Self::English => usize::from(count != 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{OutputBuffer, PhpString};

    #[test]
    fn gettext_falls_back_to_original_messages_without_catalog() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(
            builtin_gettext(
                &mut context,
                vec![Value::String(PhpString::from("Hello"))],
                RuntimeSourceSpan::default()
            )
            .expect("gettext"),
            Value::String(PhpString::from("Hello"))
        );
        assert_eq!(
            builtin_ngettext(
                &mut context,
                vec![
                    Value::String(PhpString::from("one")),
                    Value::String(PhpString::from("many")),
                    Value::Int(2),
                ],
                RuntimeSourceSpan::default()
            )
            .expect("ngettext"),
            Value::String(PhpString::from("many"))
        );
    }

    #[test]
    fn gettext_state_tracks_domain_paths_and_codesets() {
        let mut output = OutputBuffer::default();
        let mut context = BuiltinContext::new(&mut output);

        assert_eq!(
            builtin_textdomain(
                &mut context,
                vec![Value::String(PhpString::from("demo"))],
                RuntimeSourceSpan::default()
            )
            .expect("textdomain"),
            Value::string("demo")
        );
        assert_eq!(
            builtin_bindtextdomain(
                &mut context,
                vec![
                    Value::String(PhpString::from("demo")),
                    Value::String(PhpString::from("")),
                ],
                RuntimeSourceSpan::default()
            )
            .expect("bindtextdomain"),
            Value::string(".")
        );
        assert_eq!(
            builtin_bind_textdomain_codeset(
                &mut context,
                vec![
                    Value::String(PhpString::from("demo")),
                    Value::String(PhpString::from("UTF-8")),
                ],
                RuntimeSourceSpan::default()
            )
            .expect("bind_textdomain_codeset"),
            Value::string("UTF-8")
        );
    }
}
