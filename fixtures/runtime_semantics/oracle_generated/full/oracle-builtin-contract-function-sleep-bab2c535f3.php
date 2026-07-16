<?php
// oracle-probe: id=oracle-builtin-contract-function-sleep-bab2c535f3 area=builtin_contract kind=function symbol=sleep source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-sleep-bab2c535f3 failure_category=builtin_contract
$name = "sleep";
echo function_exists($name) ? "available\n" : "missing\n";
