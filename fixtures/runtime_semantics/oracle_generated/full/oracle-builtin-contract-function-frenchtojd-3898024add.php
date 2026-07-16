<?php
// oracle-probe: id=oracle-builtin-contract-function-frenchtojd-3898024add area=builtin_contract kind=function symbol=frenchtojd source=ext/calendar/calendar.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-frenchtojd-3898024add failure_category=builtin_contract requires_ref_extension=calendar
$name = "frenchtojd";
echo function_exists($name) ? "available\n" : "missing\n";
