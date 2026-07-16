<?php
// oracle-probe: id=oracle-builtin-contract-function-jdtojewish-c8e5da40d7 area=builtin_contract kind=function symbol=jdtojewish source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-jdtojewish-c8e5da40d7 failure_category=builtin_contract requires_ref_extension=calendar
$name = "jdtojewish";
echo function_exists($name) ? "available\n" : "missing\n";
