<?php
// oracle-probe: id=oracle-builtin-contract-function-class-alias-845a22774d area=builtin_contract kind=function symbol=class_alias source=Zend/zend_builtin_functions.stub.php expect=pass
// runtime-semantics: category=oracle_generated expect=pass php_ref_required=1 oracle_probe_id=oracle-builtin-contract-function-class-alias-845a22774d failure_category=builtin_contract
$name = "class_alias";
echo function_exists($name) ? "available\n" : "missing\n";
