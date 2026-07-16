<?php
// oracle-probe: id=oracle-builtin-contract-function-array-shift-a06a01338d area=builtin_contract kind=function symbol=array_shift source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-shift-a06a01338d failure_category=builtin_contract
$name = "array_shift";
echo function_exists($name) ? "available\n" : "missing\n";
