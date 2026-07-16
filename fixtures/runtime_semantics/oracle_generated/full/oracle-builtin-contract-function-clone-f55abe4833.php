<?php
// oracle-probe: id=oracle-builtin-contract-function-clone-f55abe4833 area=builtin_contract kind=function symbol=clone source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-clone-f55abe4833 failure_category=builtin_contract
$name = "clone";
echo function_exists($name) ? "available\n" : "missing\n";
