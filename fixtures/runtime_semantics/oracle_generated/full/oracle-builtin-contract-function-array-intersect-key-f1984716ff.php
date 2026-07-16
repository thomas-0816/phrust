<?php
// oracle-probe: id=oracle-builtin-contract-function-array-intersect-key-f1984716ff area=builtin_contract kind=function symbol=array_intersect_key source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-intersect-key-f1984716ff failure_category=builtin_contract
$name = "array_intersect_key";
echo function_exists($name) ? "available\n" : "missing\n";
