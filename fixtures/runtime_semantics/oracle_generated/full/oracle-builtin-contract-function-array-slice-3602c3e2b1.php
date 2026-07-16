<?php
// oracle-probe: id=oracle-builtin-contract-function-array-slice-3602c3e2b1 area=builtin_contract kind=function symbol=array_slice source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-slice-3602c3e2b1 failure_category=builtin_contract
$name = "array_slice";
echo function_exists($name) ? "available\n" : "missing\n";
