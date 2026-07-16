<?php
// oracle-probe: id=oracle-builtin-contract-function-log-ff2050babf area=builtin_contract kind=function symbol=log source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-log-ff2050babf failure_category=builtin_contract
$name = "log";
echo function_exists($name) ? "available\n" : "missing\n";
