<?php
// oracle-probe: id=oracle-builtin-contract-function-timezone-name-get-45d29082af area=builtin_contract kind=function symbol=timezone_name_get source=ext/date/php_date.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-timezone-name-get-45d29082af failure_category=builtin_contract requires_ref_extension=date
try {
    $result = \timezone_name_get();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
