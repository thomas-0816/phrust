<?php
// oracle-probe: id=oracle-builtin-contract-function-json-decode-2807856a7c area=builtin_contract kind=function symbol=json_decode source=ext/json/json.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-json-decode-2807856a7c failure_category=builtin_contract requires_ref_extension=json
$name = "json_decode";
echo function_exists($name) ? "available\n" : "missing\n";
