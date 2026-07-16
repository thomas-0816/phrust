<?php
// oracle-probe: id=oracle-builtin-contract-function-array-diff-094cb2f4f2 area=builtin_contract kind=function symbol=array_diff source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-diff-094cb2f4f2 failure_category=builtin_contract
$name = "array_diff";
echo function_exists($name) ? "available\n" : "missing\n";
