<?php
// oracle-probe: id=oracle-builtin-contract-function-gregoriantojd-818dd8bc4e area=builtin_contract kind=function symbol=gregoriantojd source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-gregoriantojd-818dd8bc4e failure_category=builtin_contract requires_ref_extension=calendar
$name = "gregoriantojd";
echo function_exists($name) ? "available\n" : "missing\n";
