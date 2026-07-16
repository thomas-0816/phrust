<?php
// oracle-probe: id=oracle-builtin-contract-function-array-multisort-e26ef38bca area=builtin_contract kind=function symbol=array_multisort source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-multisort-e26ef38bca failure_category=builtin_contract
$name = "array_multisort";
echo function_exists($name) ? "available\n" : "missing\n";
