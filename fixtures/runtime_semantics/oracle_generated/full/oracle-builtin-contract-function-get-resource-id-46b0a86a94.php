<?php
// oracle-probe: id=oracle-builtin-contract-function-get-resource-id-46b0a86a94 area=builtin_contract kind=function symbol=get_resource_id source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-resource-id-46b0a86a94 failure_category=builtin_contract
$name = "get_resource_id";
echo function_exists($name) ? "available\n" : "missing\n";
