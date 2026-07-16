<?php
// oracle-probe: id=oracle-builtin-contract-function-fgetc-e2608b2ea1 area=builtin_contract kind=function symbol=fgetc source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fgetc-e2608b2ea1 failure_category=builtin_contract
$name = "fgetc";
echo function_exists($name) ? "available\n" : "missing\n";
