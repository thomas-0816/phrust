<?php
// oracle-probe: id=oracle-builtin-behavior-function-filter-has-var-1fecc1d420 area=builtin_behavior kind=function symbol=filter_has_var source=ext/filter/filter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-filter-has-var-1fecc1d420 failure_category=builtin_behavior requires_ref_extension=filter
try {
    $result = \filter_has_var(input_type: 0, var_name: "");
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
