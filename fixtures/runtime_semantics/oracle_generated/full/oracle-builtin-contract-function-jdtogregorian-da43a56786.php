<?php
// oracle-probe: id=oracle-builtin-contract-function-jdtogregorian-da43a56786 area=builtin_contract kind=function symbol=jdtogregorian source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-jdtogregorian-da43a56786 failure_category=builtin_contract requires_ref_extension=calendar
try {
    $result = \jdtogregorian();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
