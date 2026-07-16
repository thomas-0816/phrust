<?php
// oracle-probe: id=oracle-builtin-contract-function-array-find-key-0a4971d130 area=builtin_contract kind=function symbol=array_find_key source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-find-key-0a4971d130 failure_category=builtin_contract
$name = "array_find_key";
echo function_exists($name) ? "available\n" : "missing\n";
