<?php
// oracle-probe: id=oracle-builtin-contract-function-strcmp-4c821ac5a1 area=builtin_contract kind=function symbol=strcmp source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strcmp-4c821ac5a1 failure_category=builtin_contract
$name = "strcmp";
echo function_exists($name) ? "available\n" : "missing\n";
