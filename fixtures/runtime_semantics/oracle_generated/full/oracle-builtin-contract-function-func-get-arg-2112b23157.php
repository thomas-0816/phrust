<?php
// oracle-probe: id=oracle-builtin-contract-function-func-get-arg-2112b23157 area=builtin_contract kind=function symbol=func_get_arg source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-func-get-arg-2112b23157 failure_category=builtin_contract
$name = "func_get_arg";
echo function_exists($name) ? "available\n" : "missing\n";
