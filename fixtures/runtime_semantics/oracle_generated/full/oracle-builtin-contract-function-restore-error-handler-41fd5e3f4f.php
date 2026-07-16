<?php
// oracle-probe: id=oracle-builtin-contract-function-restore-error-handler-41fd5e3f4f area=builtin_contract kind=function symbol=restore_error_handler source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-restore-error-handler-41fd5e3f4f failure_category=builtin_contract
$name = "restore_error_handler";
echo function_exists($name) ? "available\n" : "missing\n";
