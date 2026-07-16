<?php
// oracle-probe: id=oracle-builtin-contract-function-extension-loaded-1f2619aad6 area=builtin_contract kind=function symbol=extension_loaded source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-extension-loaded-1f2619aad6 failure_category=builtin_contract
$name = "extension_loaded";
echo function_exists($name) ? "available\n" : "missing\n";
