<?php
// oracle-probe: id=oracle-builtin-contract-function-jdtounix-58cdb9e421 area=builtin_contract kind=function symbol=jdtounix source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-jdtounix-58cdb9e421 failure_category=builtin_contract requires_ref_extension=calendar
try {
    $result = \jdtounix();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
