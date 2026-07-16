<?php
// oracle-probe: id=oracle-builtin-contract-function-ksort-c0caf63ae8 area=builtin_contract kind=function symbol=ksort source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-ksort-c0caf63ae8 failure_category=builtin_contract
$name = "ksort";
echo function_exists($name) ? "available\n" : "missing\n";
