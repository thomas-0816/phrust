<?php
// oracle-probe: id=oracle-builtin-contract-function-jdtogregorian-848b1967bb area=builtin_contract kind=function symbol=jdtogregorian source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-jdtogregorian-848b1967bb failure_category=builtin_contract requires_ref_extension=calendar
$name = "jdtogregorian";
echo function_exists($name) ? "available\n" : "missing\n";
