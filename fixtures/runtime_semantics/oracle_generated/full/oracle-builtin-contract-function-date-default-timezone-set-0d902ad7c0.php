<?php
// oracle-probe: id=oracle-builtin-contract-function-date-default-timezone-set-0d902ad7c0 area=builtin_contract kind=function symbol=date_default_timezone_set source=ext/date/php_date.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-date-default-timezone-set-0d902ad7c0 failure_category=builtin_contract requires_ref_extension=date
$name = "date_default_timezone_set";
echo function_exists($name) ? "available\n" : "missing\n";
