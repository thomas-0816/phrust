<?php
// oracle-probe: id=oracle-builtin-contract-function-cal-days-in-month-b386d23662 area=builtin_contract kind=function symbol=cal_days_in_month source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-cal-days-in-month-b386d23662 failure_category=builtin_contract requires_ref_extension=calendar
$name = "cal_days_in_month";
echo function_exists($name) ? "available\n" : "missing\n";
