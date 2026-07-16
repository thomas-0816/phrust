<?php
// oracle-probe: id=oracle-builtin-contract-function-get-object-vars-8dbd96e4fb area=builtin_contract kind=function symbol=get_object_vars source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-object-vars-8dbd96e4fb failure_category=builtin_contract
$name = "get_object_vars";
echo function_exists($name) ? "available\n" : "missing\n";
