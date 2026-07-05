pub(super) const ARRAY_ELEMENT_READ: &str = php_runtime::layout_stats::SOURCE_ARRAY_ELEMENT_READ;
pub(super) const ARRAY_ELEMENT_WRITE: &str = php_runtime::layout_stats::SOURCE_ARRAY_ELEMENT_WRITE;
pub(super) const BY_REF_ARGUMENT_BINDING: &str =
    php_runtime::layout_stats::SOURCE_BY_REF_ARGUMENT_BINDING;
pub(super) const CALL_ARGUMENT_SNAPSHOT: &str =
    php_runtime::layout_stats::SOURCE_CALL_ARGUMENT_SNAPSHOT;
pub(super) const CLOSURE_CAPTURE_BINDING: &str =
    php_runtime::layout_stats::SOURCE_CLOSURE_CAPTURE_BINDING;
pub(super) const FOREACH_VALUE: &str = php_runtime::layout_stats::SOURCE_FOREACH_VALUE;
pub(super) const OBJECT_PROPERTY_READ: &str =
    php_runtime::layout_stats::SOURCE_OBJECT_PROPERTY_READ;
pub(super) const OUTPUT_STRING_CONVERSION: &str =
    php_runtime::layout_stats::SOURCE_OUTPUT_STRING_CONVERSION;
pub(super) const REFERENCE_DEREFERENCE: &str =
    php_runtime::layout_stats::SOURCE_REFERENCE_DEREFERENCE;
pub(super) const RETURN_VALUE: &str = php_runtime::layout_stats::SOURCE_RETURN_VALUE;
pub(super) const STACK_REGISTER_LOCAL_MOVE: &str =
    php_runtime::layout_stats::SOURCE_STACK_REGISTER_LOCAL_MOVE;

pub(super) type Guard = php_runtime::layout_stats::LayoutSourceGuard;

pub(super) fn enter(family: &'static str) -> Guard {
    php_runtime::layout_stats::enter_layout_source_family(family)
}

pub(super) fn enter_default(family: &'static str) -> Guard {
    php_runtime::layout_stats::enter_default_layout_source_family(family)
}
