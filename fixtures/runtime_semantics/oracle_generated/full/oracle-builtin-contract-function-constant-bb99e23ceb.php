<?php
// oracle-probe: id=oracle-builtin-contract-function-constant-bb99e23ceb area=builtin_contract kind=function symbol=constant source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-constant-bb99e23ceb failure_category=builtin_contract
$name = "constant";
echo function_exists($name) ? "available\n" : "missing\n";
