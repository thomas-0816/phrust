<?php
// oracle-probe: id=oracle-builtin-contract-function-strrpos-b9b7113bc1 area=builtin_contract kind=function symbol=strrpos source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strrpos-b9b7113bc1 failure_category=builtin_contract
$name = "strrpos";
echo function_exists($name) ? "available\n" : "missing\n";
