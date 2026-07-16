<?php
// oracle-probe: id=oracle-builtin-contract-function-jdtofrench-494a3b9f68 area=builtin_contract kind=function symbol=jdtofrench source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-jdtofrench-494a3b9f68 failure_category=builtin_contract requires_ref_extension=calendar
$name = "jdtofrench";
echo function_exists($name) ? "available\n" : "missing\n";
