<?php
// oracle-probe: id=oracle-builtin-behavior-function-jdtounix-1d29e0794d area=builtin_behavior kind=function symbol=jdtounix source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-jdtounix-1d29e0794d failure_category=builtin_behavior requires_ref_extension=calendar
try {
    $result = \jdtounix(julian_day: 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
