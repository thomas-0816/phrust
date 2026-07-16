<?php
// oracle-probe: id=oracle-builtin-contract-function-get-parent-class-e30cc6058b area=builtin_contract kind=function symbol=get_parent_class source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-parent-class-e30cc6058b failure_category=builtin_contract
$name = "get_parent_class";
echo function_exists($name) ? "available\n" : "missing\n";
