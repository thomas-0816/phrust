<?php
// oracle-probe: id=oracle-builtin-contract-function-die-8c2fb8c564 area=builtin_contract kind=function symbol=die source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-die-8c2fb8c564 failure_category=builtin_contract
$name = "die";
echo function_exists($name) ? "available\n" : "missing\n";
