<?php
// oracle-probe: id=oracle-builtin-contract-function-strcasecmp-80401768f3 area=builtin_contract kind=function symbol=strcasecmp source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-strcasecmp-80401768f3 failure_category=builtin_contract
$name = "strcasecmp";
echo function_exists($name) ? "available\n" : "missing\n";
