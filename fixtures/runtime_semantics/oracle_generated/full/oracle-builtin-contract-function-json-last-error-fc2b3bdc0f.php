<?php
// oracle-probe: id=oracle-builtin-contract-function-json-last-error-fc2b3bdc0f area=builtin_contract kind=function symbol=json_last_error source=ext/json/json.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-json-last-error-fc2b3bdc0f failure_category=builtin_contract requires_ref_extension=json
$name = "json_last_error";
echo function_exists($name) ? "available\n" : "missing\n";
