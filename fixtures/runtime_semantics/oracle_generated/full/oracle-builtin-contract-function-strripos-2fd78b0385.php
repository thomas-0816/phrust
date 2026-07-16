<?php
// oracle-probe: id=oracle-builtin-contract-function-strripos-2fd78b0385 area=builtin_contract kind=function symbol=strripos source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strripos-2fd78b0385 failure_category=builtin_contract
$name = "strripos";
echo function_exists($name) ? "available\n" : "missing\n";
