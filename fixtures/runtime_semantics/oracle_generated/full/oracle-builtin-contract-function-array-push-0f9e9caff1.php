<?php
// oracle-probe: id=oracle-builtin-contract-function-array-push-0f9e9caff1 area=builtin_contract kind=function symbol=array_push source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-push-0f9e9caff1 failure_category=builtin_contract
$name = "array_push";
echo function_exists($name) ? "available\n" : "missing\n";
