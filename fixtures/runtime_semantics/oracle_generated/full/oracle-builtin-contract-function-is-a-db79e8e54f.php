<?php
// oracle-probe: id=oracle-builtin-contract-function-is-a-db79e8e54f area=builtin_contract kind=function symbol=is_a source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-is-a-db79e8e54f failure_category=builtin_contract
$name = "is_a";
echo function_exists($name) ? "available\n" : "missing\n";
