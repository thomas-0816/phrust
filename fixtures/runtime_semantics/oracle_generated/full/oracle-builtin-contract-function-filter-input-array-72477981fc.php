<?php
// oracle-probe: id=oracle-builtin-contract-function-filter-input-array-72477981fc area=builtin_contract kind=function symbol=filter_input_array source=ext/filter/filter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-filter-input-array-72477981fc failure_category=builtin_contract requires_ref_extension=filter
try {
    $result = \filter_input_array();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
