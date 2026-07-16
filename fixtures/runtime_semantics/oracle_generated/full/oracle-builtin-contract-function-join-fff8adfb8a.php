<?php
// oracle-probe: id=oracle-builtin-contract-function-join-fff8adfb8a area=builtin_contract kind=function symbol=join source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-join-fff8adfb8a failure_category=builtin_contract
$name = "join";
echo function_exists($name) ? "available\n" : "missing\n";
