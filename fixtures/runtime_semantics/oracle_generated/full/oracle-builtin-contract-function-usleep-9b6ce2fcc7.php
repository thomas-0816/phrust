<?php
// oracle-probe: id=oracle-builtin-contract-function-usleep-9b6ce2fcc7 area=builtin_contract kind=function symbol=usleep source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-usleep-9b6ce2fcc7 failure_category=builtin_contract
$name = "usleep";
echo function_exists($name) ? "available\n" : "missing\n";
