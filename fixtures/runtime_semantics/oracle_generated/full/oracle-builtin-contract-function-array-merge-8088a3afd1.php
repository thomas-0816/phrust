<?php
// oracle-probe: id=oracle-builtin-contract-function-array-merge-8088a3afd1 area=builtin_contract kind=function symbol=array_merge source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-merge-8088a3afd1 failure_category=builtin_contract
$name = "array_merge";
echo function_exists($name) ? "available\n" : "missing\n";
