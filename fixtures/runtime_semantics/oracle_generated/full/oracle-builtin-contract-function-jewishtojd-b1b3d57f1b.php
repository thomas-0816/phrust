<?php
// oracle-probe: id=oracle-builtin-contract-function-jewishtojd-b1b3d57f1b area=builtin_contract kind=function symbol=jewishtojd source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-jewishtojd-b1b3d57f1b failure_category=builtin_contract requires_ref_extension=calendar
try {
    $result = \jewishtojd();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
