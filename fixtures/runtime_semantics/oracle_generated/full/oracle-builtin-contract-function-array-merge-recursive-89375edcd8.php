<?php
// oracle-probe: id=oracle-builtin-contract-function-array-merge-recursive-89375edcd8 area=builtin_contract kind=function symbol=array_merge_recursive source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-merge-recursive-89375edcd8 failure_category=builtin_contract
$name = "array_merge_recursive";
echo function_exists($name) ? "available\n" : "missing\n";
