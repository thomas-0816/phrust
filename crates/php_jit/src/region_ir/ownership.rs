//! Ownership contracts used by executable lowering and lifetime verification.

use super::{SsaOwnership, SsaValueFact};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelperInputOwnership {
    Borrow,
    Consume,
    Retain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HelperResultOwnership {
    None,
    Owned,
    Borrowed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HelperOwnershipContract {
    pub inputs: &'static [HelperInputOwnership],
    pub result: HelperResultOwnership,
    pub may_alias_input: bool,
}

const NONE: &[HelperInputOwnership] = &[];
const CONSUME_1: &[HelperInputOwnership] = &[HelperInputOwnership::Consume];
const BORROW_1: &[HelperInputOwnership] = &[HelperInputOwnership::Borrow];
const BORROW_2: &[HelperInputOwnership] =
    &[HelperInputOwnership::Borrow, HelperInputOwnership::Borrow];
const CONSUME_BORROW_1: &[HelperInputOwnership] =
    &[HelperInputOwnership::Consume, HelperInputOwnership::Borrow];
const BORROW_3: &[HelperInputOwnership] = &[
    HelperInputOwnership::Borrow,
    HelperInputOwnership::Borrow,
    HelperInputOwnership::Borrow,
];
const BORROW_6: &[HelperInputOwnership] = &[
    HelperInputOwnership::Borrow,
    HelperInputOwnership::Borrow,
    HelperInputOwnership::Borrow,
    HelperInputOwnership::Borrow,
    HelperInputOwnership::Borrow,
    HelperInputOwnership::Borrow,
];
const CONSUME_BORROW_2: &[HelperInputOwnership] = &[
    HelperInputOwnership::Consume,
    HelperInputOwnership::Borrow,
    HelperInputOwnership::Borrow,
];

/// Ownership metadata for every stable native helper family.
#[must_use]
pub fn helper_ownership_contract(name: &str) -> Option<HelperOwnershipContract> {
    let owned = |inputs, may_alias_input| HelperOwnershipContract {
        inputs,
        result: HelperResultOwnership::Owned,
        may_alias_input,
    };
    let none = |inputs| HelperOwnershipContract {
        inputs,
        result: HelperResultOwnership::None,
        may_alias_input: false,
    };
    let borrowed = |inputs| HelperOwnershipContract {
        inputs,
        result: HelperResultOwnership::Borrowed,
        may_alias_input: false,
    };
    match name {
        "phrust_native_array_merge_recursive" | "phrust_native_array_replace_recursive" => {
            Some(owned(CONSUME_BORROW_1, false))
        }
        name if name.starts_with("phrust_native_preg_")
            || name.starts_with("phrust_native_json_")
            || matches!(
                name,
                "phrust_native_define"
                    | "phrust_native_defined"
                    | "phrust_native_constant"
                    | "phrust_native_function_exists"
                    | "phrust_native_class_exists"
                    | "phrust_native_interface_exists"
                    | "phrust_native_trait_exists"
                    | "phrust_native_enum_exists"
                    | "phrust_native_method_exists"
                    | "phrust_native_property_exists"
                    | "phrust_native_sprintf"
                    | "phrust_native_printf"
                    | "phrust_native_vsprintf"
                    | "phrust_native_vprintf"
                    | "phrust_native_number_format"
                    | "phrust_native_md5"
                    | "phrust_native_sha1"
                    | "phrust_native_crc32"
                    | "phrust_native_hash"
                    | "phrust_native_hash_hmac"
                    | "phrust_native_hash_equals"
                    | "phrust_native_base64_encode"
                    | "phrust_native_base64_decode"
                    | "phrust_native_bin2hex"
                    | "phrust_native_hex2bin"
                    | "phrust_native_quoted_printable_decode"
                    | "phrust_native_urlencode"
                    | "phrust_native_rawurlencode"
                    | "phrust_native_urldecode"
                    | "phrust_native_rawurldecode"
                    | "phrust_native_convert_uuencode"
                    | "phrust_native_convert_uudecode"
                    | "phrust_native_addcslashes"
                    | "phrust_native_stripcslashes"
                    | "phrust_native_stripslashes"
                    | "phrust_native_quotemeta"
                    | "phrust_native_pack"
                    | "phrust_native_unpack"
                    | "phrust_native_strstr"
                    | "phrust_native_stristr"
                    | "phrust_native_strrchr"
                    | "phrust_native_strpbrk"
                    | "phrust_native_substr_compare"
                    | "phrust_native_strnatcmp"
                    | "phrust_native_strnatcasecmp"
                    | "phrust_native_ucwords"
                    | "phrust_native_str_pad"
                    | "phrust_native_strtr"
                    | "phrust_native_strip_tags"
                    | "phrust_native_substr_replace"
                    | "phrust_native_str_split"
                    | "phrust_native_version_compare"
                    | "phrust_native_htmlspecialchars"
                    | "phrust_native_htmlentities"
                    | "phrust_native_html_entity_decode"
                    | "phrust_native_htmlspecialchars_decode"
                    | "phrust_native_parse_url"
                    | "phrust_native_parse_str"
                    | "phrust_native_http_build_query"
                    | "phrust_native_array_sum"
                    | "phrust_native_asort"
                    | "phrust_native_arsort"
                    | "phrust_native_ksort"
                    | "phrust_native_krsort"
                    | "phrust_native_natsort"
                    | "phrust_native_natcasesort"
                    | "phrust_native_sort"
                    | "phrust_native_rsort"
                    | "phrust_native_array_multisort"
                    | "phrust_native_random_bytes"
                    | "phrust_native_random_int"
                    | "phrust_native_rand"
                    | "phrust_native_mt_rand"
                    | "phrust_native_getrandmax"
                    | "phrust_native_mt_getrandmax"
                    | "phrust_native_array_rand"
                    | "phrust_native_shuffle"
                    | "phrust_native_spl_object_hash"
                    | "phrust_native_spl_object_id"
                    | "phrust_native_serialize"
                    | "phrust_native_unserialize"
                    | "phrust_native_get_object_vars"
                    | "phrust_native_get_mangled_object_vars"
                    | "phrust_native_get_parent_class"
                    | "phrust_native_is_subclass_of"
                    | "phrust_native_extension_loaded"
                    | "phrust_native_get_loaded_extensions"
                    | "phrust_native_ini_get"
                    | "phrust_native_ini_get_all"
                    | "phrust_native_get_cfg_var"
                    | "phrust_native_get_include_path"
                    | "phrust_native_func_num_args"
                    | "phrust_native_func_get_arg"
                    | "phrust_native_func_get_args"
                    | "phrust_native_basename"
                    | "phrust_native_dirname"
                    | "phrust_native_realpath"
                    | "phrust_native_file_exists"
                    | "phrust_native_is_file"
                    | "phrust_native_is_dir"
                    | "phrust_native_is_readable"
                    | "phrust_native_is_writable"
                    | "phrust_native_filesize"
                    | "phrust_native_filemtime"
                    | "phrust_native_file_get_contents"
                    | "phrust_native_fopen"
                    | "phrust_native_fwrite"
                    | "phrust_native_fclose"
                    | "phrust_native_fread"
                    | "phrust_native_fgets"
                    | "phrust_native_fgetc"
                    | "phrust_native_feof"
                    | "phrust_native_fflush"
                    | "phrust_native_fseek"
                    | "phrust_native_ftell"
                    | "phrust_native_ftruncate"
                    | "phrust_native_rewind"
                    | "phrust_native_stream_get_contents"
                    | "phrust_native_stream_copy_to_stream"
                    | "phrust_native_ob_start"
                    | "phrust_native_ob_get_clean"
                    | "phrust_native_ob_get_contents"
                    | "phrust_native_ob_get_flush"
                    | "phrust_native_ob_get_length"
                    | "phrust_native_ob_get_level"
                    | "phrust_native_ob_end_flush"
                    | "phrust_native_ob_end_clean"
            ) =>
        {
            Some(owned(BORROW_6, false))
        }
        "phrust_baseline_native_call_dispatch"
        | "phrust_baseline_native_builtin_dispatch"
        | "phrust_baseline_native_semantic_dispatch"
        | "phrust_jit_native_dynamic_code" => Some(owned(NONE, false)),
        "phrust_jit_native_function_resolve"
        | "phrust_native_frame_alloc"
        | "phrust_native_frame_release"
        | "phrust_native_numeric_string" => Some(none(NONE)),
        "phrust_baseline_native_unary"
        | "phrust_baseline_native_cast"
        | "phrust_native_type_predicate"
        | "phrust_native_stable_length"
        | "phrust_native_local_fetch"
        | "phrust_native_return_check"
        | "phrust_native_object_clone"
        | "phrust_native_object_cast"
        | "phrust_native_array_cast"
        | "phrust_native_int_cast"
        | "phrust_native_float_cast"
        | "phrust_native_string_cast"
        | "phrust_native_foreach_init"
        | "phrust_native_constant_fetch" => Some(owned(BORROW_1, true)),
        "phrust_baseline_native_binary"
        | "phrust_baseline_native_compare"
        | "phrust_native_array_fetch"
        | "phrust_native_array_unset"
        | "phrust_native_array_spread"
        | "phrust_native_object_clone_with" => Some(owned(BORROW_2, true)),
        "phrust_native_string_predicate" => Some(owned(BORROW_2, false)),
        "phrust_native_dynamic_property_slot" | "phrust_native_dynamic_property_test_slot" => {
            Some(borrowed(BORROW_2))
        }
        "phrust_native_local_store"
        | "phrust_native_reference_bind"
        | "phrust_native_property_fetch"
        | "phrust_native_array_insert" => Some(owned(BORROW_3, true)),
        "phrust_native_array_insert_local" => Some(owned(CONSUME_BORROW_2, true)),
        "phrust_native_property_assign" => Some(owned(BORROW_2, true)),
        "phrust_native_argument_check" => Some(owned(BORROW_1, true)),
        "phrust_native_array_new" | "phrust_native_object_new" | "phrust_native_exception_new" => {
            Some(owned(NONE, false))
        }
        "phrust_native_prepared_exception_new" => Some(owned(BORROW_1, false)),
        "phrust_native_value_release" => Some(none(CONSUME_1)),
        "phrust_native_echo"
        | "phrust_native_foreach_cleanup"
        | "phrust_native_runtime_fatal"
        | "phrust_native_execution_poll" => Some(none(BORROW_1)),
        "phrust_native_foreach_next" | "phrust_native_truthy" => Some(owned(BORROW_1, false)),
        _ => None,
    }
}

/// Whether copying this native SSA value creates another runtime owner.
#[must_use]
pub const fn value_copy_requires_retain(fact: SsaValueFact) -> bool {
    fact.has_runtime_lifecycle() && !matches!(fact.ownership, SsaOwnership::ImmortalConstant)
}

/// Whether the current SSA name still owns a runtime reference at its last use.
#[must_use]
pub const fn value_release_required(fact: SsaValueFact) -> bool {
    fact.has_runtime_lifecycle()
        && matches!(
            fact.ownership,
            SsaOwnership::Owned | SsaOwnership::AliasedReference | SsaOwnership::Unknown
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_stable_helper_declares_an_ownership_contract() {
        for helper in crate::JIT_HELPER_SYMBOLS {
            assert!(
                helper_ownership_contract(helper.name).is_some(),
                "missing ownership contract for {}",
                helper.name
            );
        }
    }
}
