<?php
// oracle-probe: id=oracle-builtin-contract-function-strncmp-e7c0f4094b area=builtin_contract kind=function symbol=strncmp source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strncmp-e7c0f4094b failure_category=builtin_contract
$name = "strncmp";
echo function_exists($name) ? "available\n" : "missing\n";
