<?php
// oracle-probe: id=oracle-builtin-contract-function-enum-exists-4257062af8 area=builtin_contract kind=function symbol=enum_exists source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-enum-exists-4257062af8 failure_category=builtin_contract
$name = "enum_exists";
echo function_exists($name) ? "available\n" : "missing\n";
