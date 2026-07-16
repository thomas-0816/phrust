<?php
// oracle-probe: id=oracle-builtin-contract-function-restore-exception-handler-b9eec0a175 area=builtin_contract kind=function symbol=restore_exception_handler source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-restore-exception-handler-b9eec0a175 failure_category=builtin_contract
$name = "restore_exception_handler";
echo function_exists($name) ? "available\n" : "missing\n";
