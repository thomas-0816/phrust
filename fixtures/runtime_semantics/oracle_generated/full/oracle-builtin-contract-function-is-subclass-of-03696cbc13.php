<?php
// oracle-probe: id=oracle-builtin-contract-function-is-subclass-of-03696cbc13 area=builtin_contract kind=function symbol=is_subclass_of source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-subclass-of-03696cbc13 failure_category=builtin_contract
$name = "is_subclass_of";
echo function_exists($name) ? "available\n" : "missing\n";
