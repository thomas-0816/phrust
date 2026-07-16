<?php
// oracle-probe: id=oracle-builtin-contract-function-get-extension-funcs-3c91c02251 area=builtin_contract kind=function symbol=get_extension_funcs source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-extension-funcs-3c91c02251 failure_category=builtin_contract
$name = "get_extension_funcs";
echo function_exists($name) ? "available\n" : "missing\n";
