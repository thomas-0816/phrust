<?php
// oracle-probe: id=oracle-builtin-contract-function-get-loaded-extensions-3ce06dedc5 area=builtin_contract kind=function symbol=get_loaded_extensions source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-get-loaded-extensions-3ce06dedc5 failure_category=builtin_contract
$name = "get_loaded_extensions";
echo function_exists($name) ? "available\n" : "missing\n";
