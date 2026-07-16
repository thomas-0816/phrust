<?php
// oracle-probe: id=oracle-builtin-contract-function-interface-exists-03fc9f84ce area=builtin_contract kind=function symbol=interface_exists source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-interface-exists-03fc9f84ce failure_category=builtin_contract
$name = "interface_exists";
echo function_exists($name) ? "available\n" : "missing\n";
