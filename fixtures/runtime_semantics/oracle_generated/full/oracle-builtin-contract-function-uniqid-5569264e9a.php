<?php
// oracle-probe: id=oracle-builtin-contract-function-uniqid-5569264e9a area=builtin_contract kind=function symbol=uniqid source=ext/standard/basic_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-uniqid-5569264e9a failure_category=builtin_contract
$name = "uniqid";
echo function_exists($name) ? "available\n" : "missing\n";
