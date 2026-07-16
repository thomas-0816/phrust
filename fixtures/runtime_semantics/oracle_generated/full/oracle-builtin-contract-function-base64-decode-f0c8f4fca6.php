<?php
// oracle-probe: id=oracle-builtin-contract-function-base64-decode-f0c8f4fca6 area=builtin_contract kind=function symbol=base64_decode source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-base64-decode-f0c8f4fca6 failure_category=builtin_contract
$name = "base64_decode";
echo function_exists($name) ? "available\n" : "missing\n";
