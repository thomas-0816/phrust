<?php
// oracle-probe: id=oracle-reference-binding-callback-callback-non-lvalue-negative-774d08a63f area=reference_binding kind=callback symbol=callback-non-lvalue-negative source=seed expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-reference-binding-callback-callback-non-lvalue-negative-774d08a63f failure_category=reference_binding
function oracle_probe_needs_ref(&$value) { echo "called\n"; }
try {
    call_user_func_array("oracle_probe_needs_ref", [1]);
} catch (Throwable $error) {
    echo get_class($error), "\n";
}
