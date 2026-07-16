<?php
// oracle-probe: id=oracle-builtin-contract-function-array-keys-9710a6cf78 area=builtin_contract kind=function symbol=array_keys source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-keys-9710a6cf78 failure_category=builtin_contract
$name = "array_keys";
echo function_exists($name) ? "available\n" : "missing\n";
