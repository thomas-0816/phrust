<?php
// oracle-probe: id=oracle-builtin-contract-function-timezone-open-12e619a9a8 area=builtin_contract kind=function symbol=timezone_open source=ext/date/php_date.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-timezone-open-12e619a9a8 failure_category=builtin_contract requires_ref_extension=date
$name = "timezone_open";
echo function_exists($name) ? "available\n" : "missing\n";
