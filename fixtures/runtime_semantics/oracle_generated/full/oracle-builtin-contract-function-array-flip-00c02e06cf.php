<?php
// oracle-probe: id=oracle-builtin-contract-function-array-flip-00c02e06cf area=builtin_contract kind=function symbol=array_flip source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-array-flip-00c02e06cf failure_category=builtin_contract
$name = "array_flip";
echo function_exists($name) ? "available\n" : "missing\n";
