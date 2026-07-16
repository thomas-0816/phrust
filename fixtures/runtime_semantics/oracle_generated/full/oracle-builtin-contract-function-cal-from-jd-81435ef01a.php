<?php
// oracle-probe: id=oracle-builtin-contract-function-cal-from-jd-81435ef01a area=builtin_contract kind=function symbol=cal_from_jd source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-cal-from-jd-81435ef01a failure_category=builtin_contract requires_ref_extension=calendar
$name = "cal_from_jd";
echo function_exists($name) ? "available\n" : "missing\n";
