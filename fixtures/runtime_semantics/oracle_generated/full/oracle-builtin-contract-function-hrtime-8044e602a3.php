<?php
// oracle-probe: id=oracle-builtin-contract-function-hrtime-8044e602a3 area=builtin_contract kind=function symbol=hrtime source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-hrtime-8044e602a3 failure_category=builtin_contract
$name = "hrtime";
echo function_exists($name) ? "available\n" : "missing\n";
