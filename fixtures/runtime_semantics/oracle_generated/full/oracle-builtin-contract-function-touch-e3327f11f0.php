<?php
// oracle-probe: id=oracle-builtin-contract-function-touch-e3327f11f0 area=builtin_contract kind=function symbol=touch source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-touch-e3327f11f0 failure_category=builtin_contract
$name = "touch";
echo function_exists($name) ? "available\n" : "missing\n";
