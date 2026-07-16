<?php
// oracle-probe: id=oracle-builtin-contract-function-ini-get-all-858190af44 area=builtin_contract kind=function symbol=ini_get_all source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ini-get-all-858190af44 failure_category=builtin_contract
$name = "ini_get_all";
echo function_exists($name) ? "available\n" : "missing\n";
