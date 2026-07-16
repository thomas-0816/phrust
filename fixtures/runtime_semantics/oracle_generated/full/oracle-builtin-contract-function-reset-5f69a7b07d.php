<?php
// oracle-probe: id=oracle-builtin-contract-function-reset-5f69a7b07d area=builtin_contract kind=function symbol=reset source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-reset-5f69a7b07d failure_category=builtin_contract
$name = "reset";
echo function_exists($name) ? "available\n" : "missing\n";
