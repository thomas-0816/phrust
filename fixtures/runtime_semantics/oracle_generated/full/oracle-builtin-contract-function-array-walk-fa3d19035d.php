<?php
// oracle-probe: id=oracle-builtin-contract-function-array-walk-fa3d19035d area=builtin_contract kind=function symbol=array_walk source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-walk-fa3d19035d failure_category=builtin_contract
$name = "array_walk";
echo function_exists($name) ? "available\n" : "missing\n";
