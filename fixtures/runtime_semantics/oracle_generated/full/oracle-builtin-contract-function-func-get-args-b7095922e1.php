<?php
// oracle-probe: id=oracle-builtin-contract-function-func-get-args-b7095922e1 area=builtin_contract kind=function symbol=func_get_args source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-func-get-args-b7095922e1 failure_category=builtin_contract
$name = "func_get_args";
echo function_exists($name) ? "available\n" : "missing\n";
