<?php
// oracle-probe: id=oracle-builtin-contract-function-jddayofweek-c438d70f84 area=builtin_contract kind=function symbol=jddayofweek source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-jddayofweek-c438d70f84 failure_category=builtin_contract requires_ref_extension=calendar
$name = "jddayofweek";
echo function_exists($name) ? "available\n" : "missing\n";
