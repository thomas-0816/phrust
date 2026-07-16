<?php
// oracle-probe: id=oracle-builtin-contract-function-array-walk-recursive-5d4edc3318 area=builtin_contract kind=function symbol=array_walk_recursive source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-walk-recursive-5d4edc3318 failure_category=builtin_contract
$name = "array_walk_recursive";
echo function_exists($name) ? "available\n" : "missing\n";
