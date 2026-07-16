<?php
// oracle-probe: id=oracle-builtin-contract-function-array-rand-07dabe027e area=builtin_contract kind=function symbol=array_rand source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-rand-07dabe027e failure_category=builtin_contract
$name = "array_rand";
echo function_exists($name) ? "available\n" : "missing\n";
