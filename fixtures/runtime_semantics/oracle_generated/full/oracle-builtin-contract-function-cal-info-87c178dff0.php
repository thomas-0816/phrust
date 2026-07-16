<?php
// oracle-probe: id=oracle-builtin-contract-function-cal-info-87c178dff0 area=builtin_contract kind=function symbol=cal_info source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-cal-info-87c178dff0 failure_category=builtin_contract requires_ref_extension=calendar
$name = "cal_info";
echo function_exists($name) ? "available\n" : "missing\n";
