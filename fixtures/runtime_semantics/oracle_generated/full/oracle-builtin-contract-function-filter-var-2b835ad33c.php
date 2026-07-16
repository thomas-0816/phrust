<?php
// oracle-probe: id=oracle-builtin-contract-function-filter-var-2b835ad33c area=builtin_contract kind=function symbol=filter_var source=ext/filter/filter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-filter-var-2b835ad33c failure_category=builtin_contract requires_ref_extension=filter
try {
    $result = \filter_var();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
