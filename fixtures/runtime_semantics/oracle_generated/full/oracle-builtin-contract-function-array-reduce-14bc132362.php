<?php
// oracle-probe: id=oracle-builtin-contract-function-array-reduce-14bc132362 area=builtin_contract kind=function symbol=array_reduce source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-reduce-14bc132362 failure_category=builtin_contract
$name = "array_reduce";
echo function_exists($name) ? "available\n" : "missing\n";
