<?php
// oracle-probe: id=oracle-builtin-contract-function-get-exception-handler-40b21de2da area=builtin_contract kind=function symbol=get_exception_handler source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-exception-handler-40b21de2da failure_category=builtin_contract
$name = "get_exception_handler";
echo function_exists($name) ? "available\n" : "missing\n";
