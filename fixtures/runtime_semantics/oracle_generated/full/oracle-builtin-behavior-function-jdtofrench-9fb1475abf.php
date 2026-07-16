<?php
// oracle-probe: id=oracle-builtin-behavior-function-jdtofrench-9fb1475abf area=builtin_behavior kind=function symbol=jdtofrench source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-jdtofrench-9fb1475abf failure_category=builtin_behavior requires_ref_extension=calendar
try {
    $result = \jdtofrench(0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
