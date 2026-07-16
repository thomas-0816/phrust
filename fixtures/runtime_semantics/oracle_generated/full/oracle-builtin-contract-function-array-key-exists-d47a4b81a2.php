<?php
// oracle-probe: id=oracle-builtin-contract-function-array-key-exists-d47a4b81a2 area=builtin_contract kind=function symbol=array_key_exists source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-key-exists-d47a4b81a2 failure_category=builtin_contract
$name = "array_key_exists";
echo function_exists($name) ? "available\n" : "missing\n";
