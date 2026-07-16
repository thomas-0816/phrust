<?php
// oracle-probe: id=oracle-builtin-contract-function-ini-set-75fc0c2842 area=builtin_contract kind=function symbol=ini_set source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ini-set-75fc0c2842 failure_category=builtin_contract
$name = "ini_set";
echo function_exists($name) ? "available\n" : "missing\n";
