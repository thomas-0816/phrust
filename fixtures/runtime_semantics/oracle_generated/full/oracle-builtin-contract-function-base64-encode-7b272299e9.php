<?php
// oracle-probe: id=oracle-builtin-contract-function-base64-encode-7b272299e9 area=builtin_contract kind=function symbol=base64_encode source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-base64-encode-7b272299e9 failure_category=builtin_contract
$name = "base64_encode";
echo function_exists($name) ? "available\n" : "missing\n";
