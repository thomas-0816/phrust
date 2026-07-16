<?php
// oracle-probe: id=oracle-builtin-behavior-function-filter-list-d85496bcd5 area=builtin_behavior kind=function symbol=filter_list source=ext/filter/filter.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-filter-list-d85496bcd5 failure_category=builtin_behavior requires_ref_extension=filter
try {
    $result = \filter_list();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
