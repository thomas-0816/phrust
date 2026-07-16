<?php
// oracle-probe: id=oracle-builtin-contract-function-json-last-error-msg-c5cbec6c4a area=builtin_contract kind=function symbol=json_last_error_msg source=ext/json/json.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-json-last-error-msg-c5cbec6c4a failure_category=builtin_contract requires_ref_extension=json
$name = "json_last_error_msg";
echo function_exists($name) ? "available\n" : "missing\n";
