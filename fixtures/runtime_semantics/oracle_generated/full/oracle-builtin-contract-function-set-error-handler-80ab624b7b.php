<?php
// oracle-probe: id=oracle-builtin-contract-function-set-error-handler-80ab624b7b area=builtin_contract kind=function symbol=set_error_handler source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-set-error-handler-80ab624b7b failure_category=builtin_contract
$name = "set_error_handler";
echo function_exists($name) ? "available\n" : "missing\n";
