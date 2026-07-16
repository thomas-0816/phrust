<?php
// oracle-probe: id=oracle-builtin-contract-function-atan-c977e9503b area=builtin_contract kind=function symbol=atan source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-atan-c977e9503b failure_category=builtin_contract
$name = "atan";
echo function_exists($name) ? "available\n" : "missing\n";
