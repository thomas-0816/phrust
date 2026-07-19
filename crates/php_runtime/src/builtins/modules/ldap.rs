//! Deterministic LDAP facade with explicit no-backend behavior.

use super::core::{argument_type_error, arity_error, assign_reference_arg, int_arg, string_arg};
use crate::builtins::{
    BuiltinCompatibility, BuiltinContext, BuiltinEntry, BuiltinResult, LdapSearchScope,
};
use crate::{
    ArrayKey, BuiltinError, ClassEntry, ClassFlags, ObjectRef, PhpArray, PhpString,
    RuntimeSourceSpan, Value, normalize_class_name,
};

const LDAP_CONNECTION_CLASS: &str = "LDAP\\Connection";
const LDAP_RESULT_CLASS: &str = "LDAP\\Result";
const LDAP_RESULT_ENTRY_CLASS: &str = "LDAP\\ResultEntry";
const LDAP_CONNECTION_ID_PROPERTY: &str = "__phrust_ldap_connection_id";
const LDAP_RESULT_ID_PROPERTY: &str = "__phrust_ldap_result_id";
const LDAP_ENTRY_ID_PROPERTY: &str = "__phrust_ldap_entry_id";
const LDAP_NO_BACKEND_ERRNO: i64 = 81;
const LDAP_ESCAPE_FILTER: i64 = 1;
const LDAP_ESCAPE_DN: i64 = 2;

pub(in crate::builtins) const ENTRIES: &[BuiltinEntry] = &[
    BuiltinEntry::new(
        "ldap_8859_to_t61",
        builtin_ldap_identity,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_add",
        builtin_ldap_mutation_bool,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_add_ext",
        builtin_ldap_mutation_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("ldap_bind", builtin_ldap_bind, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ldap_bind_ext",
        builtin_ldap_bind_ext,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("ldap_close", builtin_ldap_unbind, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ldap_compare",
        builtin_ldap_compare,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_connect",
        builtin_ldap_connect,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_count_entries",
        builtin_ldap_count_entries,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_count_references",
        builtin_ldap_count_references,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_delete",
        builtin_ldap_mutation_bool,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_delete_ext",
        builtin_ldap_mutation_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_dn2ufn",
        builtin_ldap_dn2ufn,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_err2str",
        builtin_ldap_err2str,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("ldap_errno", builtin_ldap_errno, BuiltinCompatibility::Php),
    BuiltinEntry::new("ldap_error", builtin_ldap_error, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ldap_escape",
        builtin_ldap_escape,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("ldap_exop", builtin_ldap_exop, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ldap_exop_passwd",
        builtin_ldap_exop_passwd,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_exop_refresh",
        builtin_ldap_exop_refresh,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_exop_sync",
        builtin_ldap_exop,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_exop_whoami",
        builtin_ldap_exop_whoami,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_explode_dn",
        builtin_ldap_explode_dn,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_first_attribute",
        builtin_ldap_first_attribute,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_first_entry",
        builtin_ldap_first_entry,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_first_reference",
        builtin_ldap_first_reference,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_free_result",
        builtin_ldap_free_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_get_attributes",
        builtin_ldap_get_attributes,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_get_dn",
        builtin_ldap_get_dn,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_get_entries",
        builtin_ldap_get_entries,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_get_option",
        builtin_ldap_get_option,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_get_values",
        builtin_ldap_get_values,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_get_values_len",
        builtin_ldap_get_values,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("ldap_list", builtin_ldap_list, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ldap_mod_add",
        builtin_ldap_mutation_bool,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_mod_add_ext",
        builtin_ldap_mutation_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_mod_del",
        builtin_ldap_mutation_bool,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_mod_del_ext",
        builtin_ldap_mutation_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_mod_replace",
        builtin_ldap_mutation_bool,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_mod_replace_ext",
        builtin_ldap_mutation_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_modify",
        builtin_ldap_mutation_bool,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_modify_batch",
        builtin_ldap_mutation_bool,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_next_attribute",
        builtin_ldap_next_attribute,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_next_entry",
        builtin_ldap_next_entry,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_next_reference",
        builtin_ldap_next_reference,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_parse_exop",
        builtin_ldap_parse_exop,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_parse_reference",
        builtin_ldap_parse_reference,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_parse_result",
        builtin_ldap_parse_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new("ldap_read", builtin_ldap_read, BuiltinCompatibility::Php),
    BuiltinEntry::new(
        "ldap_rename",
        builtin_ldap_mutation_bool,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_rename_ext",
        builtin_ldap_mutation_result,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_sasl_bind",
        builtin_ldap_bind,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_search",
        builtin_ldap_search,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_set_option",
        builtin_ldap_set_option,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_set_rebind_proc",
        builtin_ldap_set_rebind_proc,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_start_tls",
        builtin_ldap_start_tls,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_t61_to_8859",
        builtin_ldap_identity,
        BuiltinCompatibility::Php,
    ),
    BuiltinEntry::new(
        "ldap_unbind",
        builtin_ldap_unbind,
        BuiltinCompatibility::Php,
    ),
];

fn builtin_ldap_connect(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() > 2 {
        return Err(arity_error("ldap_connect", "zero to two arguments"));
    }
    let uri = match args.first() {
        None | Some(Value::Null) => None,
        Some(value) => Some(string_arg("ldap_connect", value)?.to_string()),
    };
    let port = args
        .get(1)
        .map(|value| int_arg("ldap_connect", value))
        .transpose()?
        .unwrap_or(389);
    let id = context.ldap_state().connect(uri, port);
    Ok(Value::Object(ldap_connection_object(id)))
}

fn builtin_ldap_unbind(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ldap_unbind", "one argument"));
    }
    let id = ldap_connection_id_arg("ldap_unbind", &args[0])?;
    Ok(Value::Bool(context.ldap_state().close(id)))
}

fn builtin_ldap_bind(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 8 {
        return Err(arity_error("ldap_bind", "one to eight arguments"));
    }
    let id = ldap_connection_id_arg("ldap_bind", &args[0])?;
    ensure_ldap_open(context, id, "ldap_bind")?;
    let bind_dn = optional_string_arg("ldap_bind", args.get(1))?.unwrap_or_default();
    let password = optional_string_arg("ldap_bind", args.get(2))?.unwrap_or_default();
    set_backend_unavailable(context, id, "ldap_bind");
    let Some(uri) = configured_live_ldap_uri(context, id) else {
        return Ok(Value::Bool(false));
    };
    Ok(Value::Bool(
        context
            .ldap_state()
            .bind_backend(id, &uri, &bind_dn, &password),
    ))
}

fn builtin_ldap_bind_ext(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    let result = builtin_ldap_bind(context, args, span)?;
    if matches!(result, Value::Bool(false)) {
        return Ok(Value::Bool(false));
    }
    let result_id = context.ldap_state().empty_result();
    Ok(Value::Object(ldap_result_object(result_id)))
}

fn builtin_ldap_list(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    builtin_ldap_query(context, args, LdapSearchScope::OneLevel, "ldap_list")
}

fn builtin_ldap_read(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    builtin_ldap_query(context, args, LdapSearchScope::Base, "ldap_read")
}

fn builtin_ldap_search(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    builtin_ldap_query(context, args, LdapSearchScope::Subtree, "ldap_search")
}

fn builtin_ldap_query(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    scope: LdapSearchScope,
    function: &'static str,
) -> BuiltinResult {
    if args.len() < 3 || args.len() > 9 {
        return Err(arity_error(function, "three to nine arguments"));
    }
    let id = ldap_connection_id_arg(function, &args[0])?;
    ensure_ldap_open(context, id, function)?;
    let base = string_arg(function, &args[1])?.to_string();
    let filter = string_arg(function, &args[2])?.to_string();
    let attributes = ldap_search_attributes(function, args.get(3))?;
    if let Some(uri) = configured_live_ldap_uri(context, id)
        && let Some(result_id) = context
            .ldap_state()
            .search_backend(id, &uri, &base, scope, &filter, attributes)
    {
        return Ok(Value::Object(ldap_result_object(result_id)));
    }
    let result_id = context.ldap_state().empty_result();
    Ok(Value::Object(ldap_result_object(result_id)))
}

fn builtin_ldap_free_result(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ldap_free_result", "one argument"));
    }
    let id = ldap_result_id_arg("ldap_free_result", &args[0])?;
    Ok(Value::Bool(context.ldap_state().free_result(id)))
}

fn builtin_ldap_count_entries(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ldap_count_entries", "two arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_count_entries", &args[0])?;
    let result_id = ldap_result_id_arg("ldap_count_entries", &args[1])?;
    let count = context.ldap_state().count_entries(result_id).unwrap_or(0);
    Ok(Value::Int(count as i64))
}

fn builtin_ldap_count_references(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ldap_count_references", "two arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_count_references", &args[0])?;
    let _ = ldap_result_id_arg("ldap_count_references", &args[1])?;
    Ok(Value::Int(0))
}

fn builtin_ldap_first_entry(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ldap_first_entry", "two arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_first_entry", &args[0])?;
    let result_id = ldap_result_id_arg("ldap_first_entry", &args[1])?;
    Ok(context
        .ldap_state()
        .first_entry(result_id)
        .map_or(Value::Bool(false), |id| {
            Value::Object(ldap_entry_object(id))
        }))
}

fn builtin_ldap_next_entry(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ldap_next_entry", "two arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_next_entry", &args[0])?;
    let entry_id = ldap_entry_id_arg("ldap_next_entry", &args[1])?;
    Ok(context
        .ldap_state()
        .next_entry(entry_id)
        .map_or(Value::Bool(false), |id| {
            Value::Object(ldap_entry_object(id))
        }))
}

fn builtin_ldap_first_reference(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ldap_first_reference", "two arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_first_reference", &args[0])?;
    let _ = ldap_result_id_arg("ldap_first_reference", &args[1])?;
    Ok(Value::Bool(false))
}

fn builtin_ldap_next_reference(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ldap_next_reference", "two arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_next_reference", &args[0])?;
    let _ = ldap_entry_id_arg("ldap_next_reference", &args[1])?;
    Ok(Value::Bool(false))
}

fn builtin_ldap_get_entries(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ldap_get_entries", "two arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_get_entries", &args[0])?;
    let result_id = ldap_result_id_arg("ldap_get_entries", &args[1])?;
    Ok(context
        .ldap_state()
        .entries_array(result_id)
        .map_or(Value::Bool(false), Value::Array))
}

fn builtin_ldap_get_dn(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ldap_get_dn", "two arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_get_dn", &args[0])?;
    let entry_id = ldap_entry_id_arg("ldap_get_dn", &args[1])?;
    Ok(context
        .ldap_state()
        .entry_dn(entry_id)
        .map_or(Value::Bool(false), Value::string))
}

fn builtin_ldap_get_attributes(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ldap_get_attributes", "two arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_get_attributes", &args[0])?;
    let entry_id = ldap_entry_id_arg("ldap_get_attributes", &args[1])?;
    Ok(Value::Array(
        context
            .ldap_state()
            .entry_attributes(entry_id)
            .unwrap_or_else(empty_count_array),
    ))
}

fn builtin_ldap_get_values(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("ldap_get_values", "three arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_get_values", &args[0])?;
    let _ = ldap_entry_id_arg("ldap_get_values", &args[1])?;
    let _ = string_arg("ldap_get_values", &args[2])?;
    Ok(Value::Bool(false))
}

fn builtin_ldap_first_attribute(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ldap_first_attribute", "two arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_first_attribute", &args[0])?;
    let _ = ldap_entry_id_arg("ldap_first_attribute", &args[1])?;
    Ok(Value::Bool(false))
}

fn builtin_ldap_next_attribute(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ldap_next_attribute", "two arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_next_attribute", &args[0])?;
    let _ = ldap_entry_id_arg("ldap_next_attribute", &args[1])?;
    Ok(Value::Bool(false))
}

fn builtin_ldap_mutation_bool(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() {
        return Err(arity_error("ldap operation", "at least one argument"));
    }
    let id = ldap_connection_id_arg("ldap operation", &args[0])?;
    ensure_ldap_open(context, id, "ldap operation")?;
    set_backend_unavailable(context, id, "ldap operation");
    Ok(Value::Bool(false))
}

fn builtin_ldap_mutation_result(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    span: RuntimeSourceSpan,
) -> BuiltinResult {
    let _ = builtin_ldap_mutation_bool(context, args, span)?;
    Ok(Value::Bool(false))
}

fn builtin_ldap_compare(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 4 || args.len() > 5 {
        return Err(arity_error("ldap_compare", "four or five arguments"));
    }
    let id = ldap_connection_id_arg("ldap_compare", &args[0])?;
    ensure_ldap_open(context, id, "ldap_compare")?;
    let _ = string_arg("ldap_compare", &args[1])?;
    let _ = string_arg("ldap_compare", &args[2])?;
    let _ = string_arg("ldap_compare", &args[3])?;
    set_backend_unavailable(context, id, "ldap_compare");
    Ok(Value::Bool(false))
}

fn builtin_ldap_set_option(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("ldap_set_option", "three arguments"));
    }
    let connection_id = nullable_ldap_connection_id_arg("ldap_set_option", &args[0])?;
    let option = int_arg("ldap_set_option", &args[1])?;
    Ok(Value::Bool(context.ldap_state().set_option(
        connection_id,
        option,
        args[2].clone(),
    )))
}

fn builtin_ldap_get_option(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 3 {
        return Err(arity_error("ldap_get_option", "two or three arguments"));
    }
    let connection_id = nullable_ldap_connection_id_arg("ldap_get_option", &args[0])?;
    let option = int_arg("ldap_get_option", &args[1])?;
    let Some(value) = context.ldap_state().option(connection_id, option) else {
        return Ok(Value::Bool(false));
    };
    assign_reference_arg(args.get(2), value);
    Ok(Value::Bool(true))
}

fn builtin_ldap_errno(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ldap_errno", "one argument"));
    }
    let id = ldap_connection_id_arg("ldap_errno", &args[0])?;
    Ok(Value::Int(context.ldap_state().errno(id)))
}

fn builtin_ldap_error(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ldap_error", "one argument"));
    }
    let id = ldap_connection_id_arg("ldap_error", &args[0])?;
    Ok(Value::string(context.ldap_state().error(id)))
}

fn builtin_ldap_err2str(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ldap_err2str", "one argument"));
    }
    Ok(Value::string(ldap_error_message(int_arg(
        "ldap_err2str",
        &args[0],
    )?)))
}

fn builtin_ldap_start_tls(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ldap_start_tls", "one argument"));
    }
    let id = ldap_connection_id_arg("ldap_start_tls", &args[0])?;
    ensure_ldap_open(context, id, "ldap_start_tls")?;
    context.ldap_state().set_connection_error(
        id,
        LDAP_NO_BACKEND_ERRNO,
        "TLS is unavailable because no LDAP backend is configured",
    );
    Ok(Value::Bool(false))
}

fn builtin_ldap_escape(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 3 {
        return Err(arity_error("ldap_escape", "one to three arguments"));
    }
    let value = string_arg("ldap_escape", &args[0])?.as_bytes().to_vec();
    let ignore = args
        .get(1)
        .map(|value| string_arg("ldap_escape", value))
        .transpose()?
        .map_or_else(Vec::new, |value| value.as_bytes().to_vec());
    let flags = args
        .get(2)
        .map(|value| int_arg("ldap_escape", value))
        .transpose()?
        .unwrap_or(0);
    Ok(Value::string(ldap_escape_bytes(&value, &ignore, flags)))
}

fn builtin_ldap_explode_dn(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ldap_explode_dn", "two arguments"));
    }
    let dn = string_arg("ldap_explode_dn", &args[0])?.to_string();
    let with_attrib = int_arg("ldap_explode_dn", &args[1])? != 0;
    let parts = dn
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| {
            if with_attrib {
                Value::string(part)
            } else {
                Value::string(part.split_once('=').map_or(part, |(_, value)| value))
            }
        })
        .collect::<Vec<_>>();
    let mut output = PhpArray::from_packed(parts);
    output.insert(string_key("count"), Value::Int(output.len() as i64));
    Ok(Value::Array(output))
}

fn builtin_ldap_dn2ufn(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ldap_dn2ufn", "one argument"));
    }
    let dn = string_arg("ldap_dn2ufn", &args[0])?.to_string();
    let ufn = dn
        .split(',')
        .map(str::trim)
        .map(|part| part.split_once('=').map_or(part, |(_, value)| value))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(Value::string(ufn))
}

fn builtin_ldap_identity(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ldap character conversion", "one argument"));
    }
    Ok(Value::String(string_arg(
        "ldap character conversion",
        &args[0],
    )?))
}

fn builtin_ldap_set_rebind_proc(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 2 {
        return Err(arity_error("ldap_set_rebind_proc", "two arguments"));
    }
    let id = ldap_connection_id_arg("ldap_set_rebind_proc", &args[0])?;
    ensure_ldap_open(context, id, "ldap_set_rebind_proc")?;
    Ok(Value::Bool(true))
}

fn builtin_ldap_parse_result(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 3 || args.len() > 6 {
        return Err(arity_error("ldap_parse_result", "three to six arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_parse_result", &args[0])?;
    let _ = ldap_result_id_arg("ldap_parse_result", &args[1])?;
    assign_reference_arg(args.get(2), Value::Int(0));
    assign_reference_arg(args.get(3), Value::string(""));
    assign_reference_arg(args.get(4), Value::string(""));
    assign_reference_arg(args.get(5), Value::Array(PhpArray::new()));
    Ok(Value::Bool(true))
}

fn builtin_ldap_parse_reference(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("ldap_parse_reference", "three arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_parse_reference", &args[0])?;
    let _ = ldap_entry_id_arg("ldap_parse_reference", &args[1])?;
    assign_reference_arg(args.get(2), Value::Array(PhpArray::new()));
    Ok(Value::Bool(false))
}

fn builtin_ldap_parse_exop(
    _context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 4 {
        return Err(arity_error("ldap_parse_exop", "two to four arguments"));
    }
    let _ = ldap_connection_id_arg("ldap_parse_exop", &args[0])?;
    let _ = ldap_result_id_arg("ldap_parse_exop", &args[1])?;
    assign_reference_arg(args.get(2), Value::Null);
    assign_reference_arg(args.get(3), Value::Null);
    Ok(Value::Bool(false))
}

fn builtin_ldap_exop(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() < 2 || args.len() > 6 {
        return Err(arity_error("ldap_exop", "two to six arguments"));
    }
    let id = ldap_connection_id_arg("ldap_exop", &args[0])?;
    ensure_ldap_open(context, id, "ldap_exop")?;
    let _ = string_arg("ldap_exop", &args[1])?;
    assign_reference_arg(args.get(4), Value::Null);
    assign_reference_arg(args.get(5), Value::Null);
    set_backend_unavailable(context, id, "ldap_exop");
    Ok(Value::Bool(false))
}

fn builtin_ldap_exop_passwd(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.is_empty() || args.len() > 5 {
        return Err(arity_error("ldap_exop_passwd", "one to five arguments"));
    }
    let id = ldap_connection_id_arg("ldap_exop_passwd", &args[0])?;
    ensure_ldap_open(context, id, "ldap_exop_passwd")?;
    set_backend_unavailable(context, id, "ldap_exop_passwd");
    Ok(Value::Bool(false))
}

fn builtin_ldap_exop_whoami(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 1 {
        return Err(arity_error("ldap_exop_whoami", "one argument"));
    }
    let id = ldap_connection_id_arg("ldap_exop_whoami", &args[0])?;
    ensure_ldap_open(context, id, "ldap_exop_whoami")?;
    set_backend_unavailable(context, id, "ldap_exop_whoami");
    Ok(Value::Bool(false))
}

fn builtin_ldap_exop_refresh(
    context: &mut BuiltinContext<'_>,
    args: crate::builtins::BuiltinArgs,
    _span: RuntimeSourceSpan,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(arity_error("ldap_exop_refresh", "three arguments"));
    }
    let id = ldap_connection_id_arg("ldap_exop_refresh", &args[0])?;
    ensure_ldap_open(context, id, "ldap_exop_refresh")?;
    let _ = string_arg("ldap_exop_refresh", &args[1])?;
    let _ = int_arg("ldap_exop_refresh", &args[2])?;
    set_backend_unavailable(context, id, "ldap_exop_refresh");
    Ok(Value::Bool(false))
}

fn optional_string_arg(
    function: &'static str,
    value: Option<&Value>,
) -> Result<Option<String>, BuiltinError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => Ok(Some(string_arg(function, value)?.to_string())),
    }
}

fn ldap_search_attributes(
    function: &'static str,
    value: Option<&Value>,
) -> Result<Vec<String>, BuiltinError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    match value {
        Value::Null => Ok(Vec::new()),
        Value::Array(attributes) => attributes
            .iter()
            .map(|(_, value)| string_arg(function, value).map(|value| value.to_string()))
            .collect(),
        value => Ok(vec![string_arg(function, value)?.to_string()]),
    }
}

fn configured_live_ldap_uri(context: &mut BuiltinContext<'_>, id: i64) -> Option<String> {
    let uri = context.ldap_state().connection_uri(id)?;
    if !context.network_requests_enabled() {
        return None;
    }
    context
        .env_value("PHRUST_LDAP_LIVE_URI")
        .is_some_and(|configured| configured == uri)
        .then_some(uri)
}

fn ensure_ldap_open(
    context: &mut BuiltinContext<'_>,
    id: i64,
    function: &'static str,
) -> Result<(), BuiltinError> {
    if context.ldap_state().is_open(id) {
        return Ok(());
    }
    Err(BuiltinError::new(
        "E_PHP_RUNTIME_VALUE",
        format!("{function}(): LDAP connection has already been closed"),
    ))
}

fn set_backend_unavailable(context: &mut BuiltinContext<'_>, id: i64, operation: &'static str) {
    context.ldap_state().set_connection_error(
        id,
        LDAP_NO_BACKEND_ERRNO,
        format!("{operation} requires an LDAP backend"),
    );
}

fn ldap_connection_id_arg(function: &'static str, value: &Value) -> Result<i64, BuiltinError> {
    object_id_arg(
        function,
        value,
        LDAP_CONNECTION_ID_PROPERTY,
        LDAP_CONNECTION_CLASS,
    )
}

fn nullable_ldap_connection_id_arg(
    function: &'static str,
    value: &Value,
) -> Result<Option<i64>, BuiltinError> {
    if matches!(value, Value::Null) {
        return Ok(None);
    }
    ldap_connection_id_arg(function, value).map(Some)
}

fn ldap_result_id_arg(function: &'static str, value: &Value) -> Result<i64, BuiltinError> {
    object_id_arg(function, value, LDAP_RESULT_ID_PROPERTY, LDAP_RESULT_CLASS)
}

fn ldap_entry_id_arg(function: &'static str, value: &Value) -> Result<i64, BuiltinError> {
    object_id_arg(
        function,
        value,
        LDAP_ENTRY_ID_PROPERTY,
        LDAP_RESULT_ENTRY_CLASS,
    )
}

fn object_id_arg(
    function: &'static str,
    value: &Value,
    property: &'static str,
    class_name: &'static str,
) -> Result<i64, BuiltinError> {
    let Value::Object(object) = value else {
        return Err(argument_type_error(function, "#1", class_name, value));
    };
    match object.get_property(property) {
        Some(Value::Int(id)) => Ok(id),
        _ => Err(argument_type_error(function, "#1", class_name, value)),
    }
}

fn ldap_connection_object(id: i64) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(
        &ldap_runtime_class("ldap\\connection"),
        LDAP_CONNECTION_CLASS,
    );
    object.set_property(LDAP_CONNECTION_ID_PROPERTY, Value::Int(id));
    object
}

fn ldap_result_object(id: i64) -> ObjectRef {
    let object =
        ObjectRef::new_with_display_name(&ldap_runtime_class("ldap\\result"), LDAP_RESULT_CLASS);
    object.set_property(LDAP_RESULT_ID_PROPERTY, Value::Int(id));
    object
}

fn ldap_entry_object(id: i64) -> ObjectRef {
    let object = ObjectRef::new_with_display_name(
        &ldap_runtime_class("ldap\\resultentry"),
        LDAP_RESULT_ENTRY_CLASS,
    );
    object.set_property(LDAP_ENTRY_ID_PROPERTY, Value::Int(id));
    object
}

fn ldap_runtime_class(name: &str) -> ClassEntry {
    ClassEntry {
        name: normalize_class_name(name).into(),
        parent: None,
        interfaces: Vec::new(),
        methods: Vec::new(),
        properties: Vec::new(),
        constants: Vec::new(),
        enum_cases: Vec::new(),
        attributes: Vec::new(),
        enum_backing_type: None,
        constructor_id: None,
        flags: ClassFlags::default(),
    }
}

fn ldap_error_message(errno: i64) -> &'static str {
    match errno {
        0 => "Success",
        1 => "Operations error",
        2 => "Protocol error",
        32 => "No such object",
        34 => "Invalid DN syntax",
        49 => "Invalid credentials",
        80 => "Other (e.g., implementation specific) error",
        81 => "Can't contact LDAP server",
        -1 => "Can't contact LDAP server",
        _ => "Unknown error",
    }
}

fn ldap_escape_bytes(value: &[u8], ignore: &[u8], flags: i64) -> Vec<u8> {
    if value.is_empty() {
        return Vec::new();
    }
    let mut map = [false; 256];
    let mut have_charlist = false;
    if flags & LDAP_ESCAPE_FILTER != 0 {
        have_charlist = true;
        for byte in b"\\*()\0" {
            map[*byte as usize] = true;
        }
    }
    if flags & LDAP_ESCAPE_DN != 0 {
        have_charlist = true;
        for byte in b"\\,=+<>;\"#\r" {
            map[*byte as usize] = true;
        }
    }
    if !have_charlist {
        map.fill(true);
    }
    for byte in ignore {
        map[*byte as usize] = false;
    }

    let mut output = Vec::with_capacity(value.len());
    for (index, byte) in value.iter().copied().enumerate() {
        let dn_boundary_space =
            flags & LDAP_ESCAPE_DN != 0 && byte == b' ' && (index == 0 || index + 1 == value.len());
        if map[byte as usize] || dn_boundary_space {
            output.push(b'\\');
            output.push(nibble_hex(byte >> 4));
            output.push(nibble_hex(byte & 0x0f));
        } else {
            output.push(byte);
        }
    }
    output
}

fn nibble_hex(value: u8) -> u8 {
    match value {
        0..=9 => b'0' + value,
        _ => b'a' + (value - 10),
    }
}

fn empty_count_array() -> PhpArray {
    let mut array = PhpArray::new();
    array.insert(string_key("count"), Value::Int(0));
    array
}

fn string_key(key: &str) -> ArrayKey {
    ArrayKey::String(PhpString::from(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::OutputBuffer;

    fn call(context: &mut BuiltinContext<'_>, name: &str, args: crate::builtins::BuiltinArgs) -> Value {
        ENTRIES
            .iter()
            .find(|entry| entry.name() == name)
            .expect("entry")
            .function()(context, args, RuntimeSourceSpan::default())
        .expect("builtin ok")
    }

    #[test]
    fn ldap_facade_tracks_options_errors_and_empty_results() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        let connection = call(
            &mut context,
            "ldap_connect",
            vec![Value::string("ldap://127.0.0.1"), Value::Int(3389)],
        );
        assert!(matches!(connection, Value::Object(_)));

        assert_eq!(
            call(
                &mut context,
                "ldap_set_option",
                vec![connection.clone(), Value::Int(17), Value::Int(3)]
            ),
            Value::Bool(true)
        );
        let option = crate::ReferenceCell::new(Value::Null);
        assert_eq!(
            call(
                &mut context,
                "ldap_get_option",
                vec![
                    connection.clone(),
                    Value::Int(17),
                    Value::Reference(option.clone())
                ],
            ),
            Value::Bool(true)
        );
        assert_eq!(option.get(), Value::Int(3));

        let result = call(
            &mut context,
            "ldap_search",
            vec![
                connection.clone(),
                Value::string("dc=example,dc=org"),
                Value::string("(uid=missing)"),
            ],
        );
        assert!(matches!(result, Value::Object(_)));
        assert_eq!(
            call(
                &mut context,
                "ldap_count_entries",
                vec![connection.clone(), result.clone()]
            ),
            Value::Int(0)
        );
        assert_eq!(
            call(
                &mut context,
                "ldap_first_entry",
                vec![connection.clone(), result.clone()]
            ),
            Value::Bool(false)
        );

        assert_eq!(
            call(&mut context, "ldap_bind", vec![connection.clone()]),
            Value::Bool(false)
        );
        assert_eq!(
            call(&mut context, "ldap_errno", vec![connection.clone()]),
            Value::Int(81)
        );
        assert!(matches!(
            call(&mut context, "ldap_error", vec![connection.clone()]),
            Value::String(_)
        ));
    }

    #[test]
    fn ldap_escape_matches_php_hex_rules_for_filter_and_dn() {
        let mut output = OutputBuffer::new();
        let mut context = BuiltinContext::new(&mut output);
        assert_eq!(
            call(
                &mut context,
                "ldap_escape",
                vec![Value::string("a*(b)\\c"), Value::string(""), Value::Int(1)],
            ),
            Value::string("a\\2a\\28b\\29\\5cc")
        );
        assert_eq!(
            call(
                &mut context,
                "ldap_escape",
                vec![
                    Value::string(" cn=admin "),
                    Value::string(""),
                    Value::Int(2)
                ],
            ),
            Value::string("\\20cn\\3dadmin\\20")
        );
    }
}
