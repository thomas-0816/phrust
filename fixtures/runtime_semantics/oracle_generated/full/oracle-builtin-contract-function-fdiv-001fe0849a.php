<?php
// oracle-probe: id=oracle-builtin-contract-function-fdiv-001fe0849a area=builtin_contract kind=function symbol=fdiv source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fdiv-001fe0849a failure_category=builtin_contract
$name = "fdiv";
echo function_exists($name) ? "available\n" : "missing\n";
