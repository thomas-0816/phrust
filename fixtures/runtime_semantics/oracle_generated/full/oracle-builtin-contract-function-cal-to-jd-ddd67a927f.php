<?php
// oracle-probe: id=oracle-builtin-contract-function-cal-to-jd-ddd67a927f area=builtin_contract kind=function symbol=cal_to_jd source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-cal-to-jd-ddd67a927f failure_category=builtin_contract requires_ref_extension=calendar
$name = "cal_to_jd";
echo function_exists($name) ? "available\n" : "missing\n";
