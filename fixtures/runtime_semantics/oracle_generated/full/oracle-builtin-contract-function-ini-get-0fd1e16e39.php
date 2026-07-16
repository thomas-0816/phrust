<?php
// oracle-probe: id=oracle-builtin-contract-function-ini-get-0fd1e16e39 area=builtin_contract kind=function symbol=ini_get source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ini-get-0fd1e16e39 failure_category=builtin_contract
$name = "ini_get";
echo function_exists($name) ? "available\n" : "missing\n";
