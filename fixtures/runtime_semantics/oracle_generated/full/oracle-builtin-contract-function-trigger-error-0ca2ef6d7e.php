<?php
// oracle-probe: id=oracle-builtin-contract-function-trigger-error-0ca2ef6d7e area=builtin_contract kind=function symbol=trigger_error source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-trigger-error-0ca2ef6d7e failure_category=builtin_contract
$name = "trigger_error";
echo function_exists($name) ? "available\n" : "missing\n";
