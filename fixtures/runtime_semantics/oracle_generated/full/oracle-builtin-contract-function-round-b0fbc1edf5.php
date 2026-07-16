<?php
// oracle-probe: id=oracle-builtin-contract-function-round-b0fbc1edf5 area=builtin_contract kind=function symbol=round source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-round-b0fbc1edf5 failure_category=builtin_contract
$name = "round";
echo function_exists($name) ? "available\n" : "missing\n";
