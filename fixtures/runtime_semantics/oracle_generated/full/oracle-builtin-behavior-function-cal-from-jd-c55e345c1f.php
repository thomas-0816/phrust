<?php
// oracle-probe: id=oracle-builtin-behavior-function-cal-from-jd-c55e345c1f area=builtin_behavior kind=function symbol=cal_from_jd source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-behavior-function-cal-from-jd-c55e345c1f failure_category=builtin_behavior requires_ref_extension=calendar
try {
    $result = \cal_from_jd(0, 0);
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
