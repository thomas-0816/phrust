<?php
// oracle-probe: id=oracle-builtin-contract-function-array-pop-895a887afe area=builtin_contract kind=function symbol=array_pop source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-pop-895a887afe failure_category=builtin_contract
$name = "array_pop";
echo function_exists($name) ? "available\n" : "missing\n";
