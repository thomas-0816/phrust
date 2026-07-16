<?php
// oracle-probe: id=oracle-builtin-contract-function-json-encode-04346b96d6 area=builtin_contract kind=function symbol=json_encode source=ext/json/json.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=0 php_ref_optional_reason=missing_reference_extension oracle_probe_id=oracle-builtin-contract-function-json-encode-04346b96d6 failure_category=builtin_contract requires_ref_extension=json
$name = "json_encode";
echo function_exists($name) ? "available\n" : "missing\n";
