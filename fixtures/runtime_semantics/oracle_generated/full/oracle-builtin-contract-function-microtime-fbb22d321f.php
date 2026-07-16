<?php
// oracle-probe: id=oracle-builtin-contract-function-microtime-fbb22d321f area=builtin_contract kind=function symbol=microtime source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-microtime-fbb22d321f failure_category=builtin_contract
$name = "microtime";
echo function_exists($name) ? "available\n" : "missing\n";
