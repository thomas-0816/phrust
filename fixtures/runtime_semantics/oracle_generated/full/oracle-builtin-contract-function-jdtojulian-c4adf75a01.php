<?php
// oracle-probe: id=oracle-builtin-contract-function-jdtojulian-c4adf75a01 area=builtin_contract kind=function symbol=jdtojulian source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-jdtojulian-c4adf75a01 failure_category=builtin_contract requires_ref_extension=calendar
$name = "jdtojulian";
echo function_exists($name) ? "available\n" : "missing\n";
