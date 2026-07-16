<?php
// oracle-probe: id=oracle-builtin-contract-function-cal-to-jd-57b396c009 area=builtin_contract kind=function symbol=cal_to_jd source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-cal-to-jd-57b396c009 failure_category=builtin_contract requires_ref_extension=calendar
try {
    $result = \cal_to_jd();
    echo "return:\n";
    var_dump($result);
} catch (Throwable $error) {
    echo "throw:", get_class($error), ":", $error->getMessage(), "\n";
}
