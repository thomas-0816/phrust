<?php
// oracle-probe: id=oracle-builtin-contract-function-date-default-timezone-get-5fe2eb3335 area=builtin_contract kind=function symbol=date_default_timezone_get source=ext/date/php_date.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-date-default-timezone-get-5fe2eb3335 failure_category=builtin_contract requires_ref_extension=date
try {
    $result = \date_default_timezone_get(__phrust_probe_unknown: 1);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
