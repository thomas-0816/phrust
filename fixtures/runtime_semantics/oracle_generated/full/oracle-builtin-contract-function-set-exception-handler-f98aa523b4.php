<?php
// oracle-probe: id=oracle-builtin-contract-function-set-exception-handler-f98aa523b4 area=builtin_contract kind=function symbol=set_exception_handler source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-set-exception-handler-f98aa523b4 failure_category=builtin_contract
$name = "set_exception_handler";
echo function_exists($name) ? "available\n" : "missing\n";
