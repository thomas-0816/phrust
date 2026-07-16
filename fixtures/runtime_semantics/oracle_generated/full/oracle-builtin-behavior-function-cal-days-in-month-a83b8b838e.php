<?php
// oracle-probe: id=oracle-builtin-behavior-function-cal-days-in-month-a83b8b838e area=builtin_behavior kind=function symbol=cal_days_in_month source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-cal-days-in-month-a83b8b838e failure_category=builtin_behavior requires_ref_extension=calendar
try {
    $result = \cal_days_in_month([], 0, 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
