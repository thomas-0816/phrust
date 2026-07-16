<?php
// oracle-probe: id=oracle-builtin-contract-function-checkdate-b41dd6e21c area=builtin_contract kind=function symbol=checkdate source=ext/date/php_date.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-checkdate-b41dd6e21c failure_category=builtin_contract requires_ref_extension=date
$name = "checkdate";
echo function_exists($name) ? "available\n" : "missing\n";
