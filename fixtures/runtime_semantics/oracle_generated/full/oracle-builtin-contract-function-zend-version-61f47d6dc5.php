<?php
// oracle-probe: id=oracle-builtin-contract-function-zend-version-61f47d6dc5 area=builtin_contract kind=function symbol=zend_version source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-zend-version-61f47d6dc5 failure_category=builtin_contract
$name = "zend_version";
echo function_exists($name) ? "available\n" : "missing\n";
