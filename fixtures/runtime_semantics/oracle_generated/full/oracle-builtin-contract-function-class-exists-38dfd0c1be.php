<?php
// oracle-probe: id=oracle-builtin-contract-function-class-exists-38dfd0c1be area=builtin_contract kind=function symbol=class_exists source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-class-exists-38dfd0c1be failure_category=builtin_contract
$name = "class_exists";
echo function_exists($name) ? "available\n" : "missing\n";
