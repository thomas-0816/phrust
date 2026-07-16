<?php
// oracle-probe: id=oracle-builtin-contract-function-func-num-args-61d52b6027 area=builtin_contract kind=function symbol=func_num_args source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-func-num-args-61d52b6027 failure_category=builtin_contract
$name = "func_num_args";
echo function_exists($name) ? "available\n" : "missing\n";
