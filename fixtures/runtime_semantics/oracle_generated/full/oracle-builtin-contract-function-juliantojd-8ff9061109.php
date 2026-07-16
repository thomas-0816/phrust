<?php
// oracle-probe: id=oracle-builtin-contract-function-juliantojd-8ff9061109 area=builtin_contract kind=function symbol=juliantojd source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-juliantojd-8ff9061109 failure_category=builtin_contract requires_ref_extension=calendar
try {
    $result = \juliantojd();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
