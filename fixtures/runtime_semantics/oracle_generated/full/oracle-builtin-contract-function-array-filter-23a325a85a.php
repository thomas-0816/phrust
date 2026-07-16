<?php
// oracle-probe: id=oracle-builtin-contract-function-array-filter-23a325a85a area=builtin_contract kind=function symbol=array_filter source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-filter-23a325a85a failure_category=builtin_contract
$name = "array_filter";
echo function_exists($name) ? "available\n" : "missing\n";
