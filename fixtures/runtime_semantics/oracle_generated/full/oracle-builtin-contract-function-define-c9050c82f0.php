<?php
// oracle-probe: id=oracle-builtin-contract-function-define-c9050c82f0 area=builtin_contract kind=function symbol=define source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-define-c9050c82f0 failure_category=builtin_contract
$name = "define";
echo function_exists($name) ? "available\n" : "missing\n";
