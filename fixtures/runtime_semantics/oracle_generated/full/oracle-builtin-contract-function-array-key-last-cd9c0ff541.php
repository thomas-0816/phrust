<?php
// oracle-probe: id=oracle-builtin-contract-function-array-key-last-cd9c0ff541 area=builtin_contract kind=function symbol=array_key_last source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-key-last-cd9c0ff541 failure_category=builtin_contract
$name = "array_key_last";
echo function_exists($name) ? "available\n" : "missing\n";
