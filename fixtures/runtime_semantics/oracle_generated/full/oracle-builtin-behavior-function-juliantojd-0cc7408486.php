<?php
// oracle-probe: id=oracle-builtin-behavior-function-juliantojd-0cc7408486 area=builtin_behavior kind=function symbol=juliantojd source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-juliantojd-0cc7408486 failure_category=builtin_behavior requires_ref_extension=calendar
try {
    $result = \juliantojd(month: 0, day: 0, year: 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
