<?php
// oracle-probe: id=oracle-builtin-contract-function-date-interval-format-a219d28d5e area=builtin_contract kind=function symbol=date_interval_format source=ext/date/php_date.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-date-interval-format-a219d28d5e failure_category=builtin_contract requires_ref_extension=date
try {
    $result = \date_interval_format();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
