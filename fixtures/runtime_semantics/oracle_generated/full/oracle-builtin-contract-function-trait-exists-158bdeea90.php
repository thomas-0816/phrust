<?php
// oracle-probe: id=oracle-builtin-contract-function-trait-exists-158bdeea90 area=builtin_contract kind=function symbol=trait_exists source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-trait-exists-158bdeea90 failure_category=builtin_contract
$name = "trait_exists";
echo function_exists($name) ? "available\n" : "missing\n";
