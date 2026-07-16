<?php
// oracle-probe: id=oracle-builtin-contract-function-get-class-vars-4ae5f47f78 area=builtin_contract kind=function symbol=get_class_vars source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-class-vars-4ae5f47f78 failure_category=builtin_contract
$name = "get_class_vars";
echo function_exists($name) ? "available\n" : "missing\n";
