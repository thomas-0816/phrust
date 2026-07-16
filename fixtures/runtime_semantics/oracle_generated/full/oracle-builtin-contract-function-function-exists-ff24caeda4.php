<?php
// oracle-probe: id=oracle-builtin-contract-function-function-exists-ff24caeda4 area=builtin_contract kind=function symbol=function_exists source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-function-exists-ff24caeda4 failure_category=builtin_contract
$name = "function_exists";
echo function_exists($name) ? "available\n" : "missing\n";
