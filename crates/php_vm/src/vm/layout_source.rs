pub(super) const ARRAY_ELEMENT_READ: php_runtime::experimental::layout_stats::LayoutSourceFamily =
    php_runtime::experimental::layout_stats::SOURCE_ARRAY_ELEMENT_READ;
pub(super) const ARRAY_ELEMENT_WRITE: php_runtime::experimental::layout_stats::LayoutSourceFamily =
    php_runtime::experimental::layout_stats::SOURCE_ARRAY_ELEMENT_WRITE;
pub(super) const BUILTIN_BODY: php_runtime::experimental::layout_stats::LayoutSourceFamily =
    php_runtime::experimental::layout_stats::SOURCE_BUILTIN_BODY;
pub(super) const BY_REF_ARGUMENT_BINDING:
    php_runtime::experimental::layout_stats::LayoutSourceFamily =
    php_runtime::experimental::layout_stats::SOURCE_BY_REF_ARGUMENT_BINDING;
pub(super) const CALL_ARGUMENT_SNAPSHOT:
    php_runtime::experimental::layout_stats::LayoutSourceFamily =
    php_runtime::experimental::layout_stats::SOURCE_CALL_ARGUMENT_SNAPSHOT;
pub(super) const CLOSURE_CAPTURE_BINDING:
    php_runtime::experimental::layout_stats::LayoutSourceFamily =
    php_runtime::experimental::layout_stats::SOURCE_CLOSURE_CAPTURE_BINDING;
pub(super) const FOREACH_VALUE: php_runtime::experimental::layout_stats::LayoutSourceFamily =
    php_runtime::experimental::layout_stats::SOURCE_FOREACH_VALUE;
pub(super) const GC_ROOT_SCAN: php_runtime::experimental::layout_stats::LayoutSourceFamily =
    php_runtime::experimental::layout_stats::SOURCE_GC_ROOT_SCAN;
pub(super) const OBJECT_PROPERTY_READ: php_runtime::experimental::layout_stats::LayoutSourceFamily =
    php_runtime::experimental::layout_stats::SOURCE_OBJECT_PROPERTY_READ;
pub(super) const OUTPUT_STRING_CONVERSION:
    php_runtime::experimental::layout_stats::LayoutSourceFamily =
    php_runtime::experimental::layout_stats::SOURCE_OUTPUT_STRING_CONVERSION;
pub(super) const REFERENCE_DEREFERENCE:
    php_runtime::experimental::layout_stats::LayoutSourceFamily =
    php_runtime::experimental::layout_stats::SOURCE_REFERENCE_DEREFERENCE;
pub(super) const RETURN_VALUE: php_runtime::experimental::layout_stats::LayoutSourceFamily =
    php_runtime::experimental::layout_stats::SOURCE_RETURN_VALUE;
pub(super) const STACK_REGISTER_LOCAL_MOVE:
    php_runtime::experimental::layout_stats::LayoutSourceFamily =
    php_runtime::experimental::layout_stats::SOURCE_STACK_REGISTER_LOCAL_MOVE;

pub(super) type Guard = php_runtime::experimental::layout_stats::LayoutSourceGuard;

pub(super) fn enter(family: php_runtime::experimental::layout_stats::LayoutSourceFamily) -> Guard {
    php_runtime::experimental::layout_stats::enter_layout_source_family(family)
}

pub(super) fn enter_default(
    family: php_runtime::experimental::layout_stats::LayoutSourceFamily,
) -> Guard {
    php_runtime::experimental::layout_stats::enter_default_layout_source_family(family)
}
