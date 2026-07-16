<?php
// oracle-probe: id=oracle-builtin-contract-function-get-resource-type-a8e7aa7969 area=builtin_contract kind=function symbol=get_resource_type source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-resource-type-a8e7aa7969 failure_category=builtin_contract
$name = "get_resource_type";
echo function_exists($name) ? "available\n" : "missing\n";
