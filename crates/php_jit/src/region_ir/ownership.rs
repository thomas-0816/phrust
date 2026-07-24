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
    match name {
        name if name.starts_with("phrust_native_preg_")
            || name.starts_with("phrust_native_json_")
            || matches!(
                name,
                "phrust_native_define"
                    | "phrust_native_defined"
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
                    | "phrust_native_basename"
                    | "phrust_native_dirname"
                    | "phrust_native_realpath"
                    | "phrust_native_file_exists"
                    | "phrust_native_fopen"
                    | "phrust_native_fwrite"
                    | "phrust_native_fclose"
            ) =>
        {
            Some(owned(BORROW_6, false))
        }
        "phrust_native_include" => Some(owned(BORROW_1, false)),
        "phrust_jit_native_call_dispatch"
        | "phrust_baseline_native_builtin_dispatch"
        | "phrust_jit_native_semantic_dispatch"
        | "phrust_jit_native_dynamic_code" => Some(owned(NONE, false)),
        "phrust_jit_native_function_resolve"
        | "phrust_native_frame_alloc"
        | "phrust_native_frame_release" => Some(none(NONE)),
        "phrust_native_unary"
        | "phrust_native_cast"
        | "phrust_native_type_predicate"
        | "phrust_native_stable_length"
        | "phrust_native_local_fetch"
        | "phrust_native_return_check"
        | "phrust_native_object_clone"
        | "phrust_native_foreach_init"
        | "phrust_native_constant_fetch" => Some(owned(BORROW_1, true)),
        "phrust_native_binary"
        | "phrust_native_compare"
        | "phrust_native_array_fetch"
        | "phrust_native_array_unset"
        | "phrust_native_array_spread"
        | "phrust_native_object_clone_with" => Some(owned(BORROW_2, true)),
        "phrust_native_string_predicate" => Some(owned(BORROW_2, false)),
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
