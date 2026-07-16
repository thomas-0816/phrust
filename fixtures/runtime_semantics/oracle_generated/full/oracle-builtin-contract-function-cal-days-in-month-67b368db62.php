<?php
// oracle-probe: id=oracle-builtin-contract-function-cal-days-in-month-67b368db62 area=builtin_contract kind=function symbol=cal_days_in_month source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-cal-days-in-month-67b368db62 failure_category=builtin_contract requires_ref_extension=calendar
try {
    $result = \cal_days_in_month();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
