<?php
// oracle-probe: id=oracle-builtin-contract-function-user-error-4f74d7ca3c area=builtin_contract kind=function symbol=user_error source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-user-error-4f74d7ca3c failure_category=builtin_contract
$name = "user_error";
echo function_exists($name) ? "available\n" : "missing\n";
