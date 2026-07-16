<?php
// oracle-probe: id=oracle-builtin-contract-function-stripos-d59f0b74b7 area=builtin_contract kind=function symbol=stripos source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-stripos-d59f0b74b7 failure_category=builtin_contract
$name = "stripos";
echo function_exists($name) ? "available\n" : "missing\n";
