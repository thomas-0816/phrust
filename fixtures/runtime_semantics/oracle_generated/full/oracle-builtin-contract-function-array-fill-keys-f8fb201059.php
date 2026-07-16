<?php
// oracle-probe: id=oracle-builtin-contract-function-array-fill-keys-f8fb201059 area=builtin_contract kind=function symbol=array_fill_keys source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-fill-keys-f8fb201059 failure_category=builtin_contract
$name = "array_fill_keys";
echo function_exists($name) ? "available\n" : "missing\n";
