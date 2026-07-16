<?php
// oracle-probe: id=oracle-builtin-contract-function-strtolower-ff6857a137 area=builtin_contract kind=function symbol=strtolower source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strtolower-ff6857a137 failure_category=builtin_contract
$name = "strtolower";
echo function_exists($name) ? "available\n" : "missing\n";
