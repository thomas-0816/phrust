<?php
// oracle-probe: id=oracle-builtin-contract-function-method-exists-a21235f603 area=builtin_contract kind=function symbol=method_exists source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-method-exists-a21235f603 failure_category=builtin_contract
$name = "method_exists";
echo function_exists($name) ? "available\n" : "missing\n";
