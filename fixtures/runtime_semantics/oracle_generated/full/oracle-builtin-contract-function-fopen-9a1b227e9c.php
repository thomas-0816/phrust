<?php
// oracle-probe: id=oracle-builtin-contract-function-fopen-9a1b227e9c area=builtin_contract kind=function symbol=fopen source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-fopen-9a1b227e9c failure_category=builtin_contract
$name = "fopen";
echo function_exists($name) ? "available\n" : "missing\n";
